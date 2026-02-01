use geaflow_runtime::distributed::driver::DistributedDriver;
use geaflow_runtime::distributed::worker::{run_worker, WorkerConfig};
use geaflow_runtime::plan::job_spec::{
    AlgorithmSpec, CheckpointSpec, FileSource, GraphSpec, JobMode, JobSpec,
};
use geaflow_runtime::scheduler::cycle_scheduler::CycleScheduler;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

fn free_local_addr() -> SocketAddr {
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr
}

#[tokio::test]
async fn test_fault_injection_worker_crash_fail_fast() {
    let w1_addr = free_local_addr();
    let w2_addr = free_local_addr();

    let w1_dir = tempfile::tempdir().unwrap();
    let w2_dir = tempfile::tempdir().unwrap();

    let w1 = tokio::spawn(run_worker(WorkerConfig {
        listen_addr: w1_addr,
        state_dir: PathBuf::from(w1_dir.path()),
        master_addr: None,
    }));
    let w2 = tokio::spawn(run_worker(WorkerConfig {
        listen_addr: w2_addr,
        state_dir: PathBuf::from(w2_dir.path()),
        master_addr: None,
    }));

    let data_dir = tempfile::tempdir().unwrap();
    let vertices_path = data_dir.path().join("v.csv");
    let edges_path = data_dir.path().join("e.csv");
    std::fs::write(&vertices_path, "1\n2\n3\n").unwrap();
    std::fs::write(&edges_path, "1,2,0\n2,1,0\n2,3,0\n3,2,0\n").unwrap();

    let job = JobSpec {
        job_id: "job_fail".to_string(),
        name: "wcc".to_string(),
        mode: JobMode::Distributed,
        graph: GraphSpec {
            vertices: FileSource::Csv {
                path: vertices_path.to_string_lossy().to_string(),
            },
            edges: FileSource::Csv {
                path: edges_path.to_string_lossy().to_string(),
            },
        },
        algorithm: AlgorithmSpec::Wcc { iterations: 100 },
        checkpoint: CheckpointSpec {
            enabled: false,
            interval_iters: 0,
            base_dir: "/tmp/geaflow-checkpoints".to_string(),
        },
    };

    let mut driver = DistributedDriver::connect(&[w1_addr, w2_addr])
        .await
        .unwrap();

    let vertices =
        geaflow_runtime::io::file::read_vertices_u64_u64_id_default(&vertices_path).unwrap();
    let edges = geaflow_runtime::io::file::read_edges_u64_u8(&edges_path, 0).unwrap();
    let vertices: Vec<(Vec<u8>, Vec<u8>)> = vertices
        .into_iter()
        .map(|v| {
            (
                bincode::serialize(&v.id).unwrap(),
                bincode::serialize(&v.value).unwrap(),
            )
        })
        .collect();
    let edges: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = edges
        .into_iter()
        .map(|e| {
            (
                bincode::serialize(&e.src_id).unwrap(),
                bincode::serialize(&e.target_id).unwrap(),
                bincode::serialize(&e.value).unwrap(),
            )
        })
        .collect();

    driver.load_graph(vertices, edges).await.unwrap();
    driver
        .set_algorithm("wcc".to_string(), 100, Vec::new())
        .await
        .unwrap();

    w2.abort();

    let result = CycleScheduler::run(&mut driver, &job).await;
    assert!(result.is_err());

    let _ = driver.shutdown().await;
    w1.abort();
}
