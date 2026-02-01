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

fn dec<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> T {
    bincode::deserialize(bytes).unwrap()
}

#[tokio::test]
async fn test_distributed_checkpoint_restore_vertices() {
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

    let checkpoint_dir = tempfile::tempdir().unwrap();
    let job = JobSpec {
        job_id: "job_ckpt".to_string(),
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
        algorithm: AlgorithmSpec::Wcc { iterations: 2 },
        checkpoint: CheckpointSpec {
            enabled: true,
            interval_iters: 1,
            base_dir: checkpoint_dir.path().to_string_lossy().to_string(),
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
        .set_algorithm("wcc".to_string(), 2, Vec::new())
        .await
        .unwrap();

    CycleScheduler::run(&mut driver, &job).await.unwrap();

    let cp1_dir = std::path::Path::new(&job.checkpoint.base_dir)
        .join(&job.job_id)
        .join("cp_1");
    driver.load_checkpoint_all(&cp1_dir).await.unwrap();
    let mut vertices = driver.fetch_vertices().await.unwrap();
    vertices.sort_by(|a, b| a.0.cmp(&b.0));

    let decoded: Vec<(u64, u64)> = vertices
        .into_iter()
        .map(|(id, value)| (dec::<u64>(&id), dec::<u64>(&value)))
        .collect();
    assert_eq!(decoded, vec![(1, 1), (2, 2), (3, 3)]);

    driver.shutdown().await.unwrap();
    w1.abort();
    w2.abort();
}
