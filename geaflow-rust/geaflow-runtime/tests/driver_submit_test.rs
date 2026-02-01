use geaflow_runtime::distributed::driver_service::{DriverService, DriverServiceConfig};
use geaflow_runtime::distributed::protocol::{
    framed, recv_msg, send_msg, ClientToDriver, DriverToClient,
};
use geaflow_runtime::distributed::worker::{run_worker, WorkerConfig};
use geaflow_runtime::plan::job_spec::{
    AlgorithmSpec, CheckpointSpec, FileSource, GraphSpec, JobMode, JobSpec,
};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration};

fn free_local_addr() -> SocketAddr {
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr
}

fn enc<T: serde::Serialize>(v: &T) -> Vec<u8> {
    bincode::serialize(v).unwrap()
}

fn dec<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> T {
    bincode::deserialize(bytes).unwrap()
}

#[tokio::test]
async fn test_submit_job_via_driver_service() {
    let w1_addr = free_local_addr();
    let w2_addr = free_local_addr();
    let driver_addr = free_local_addr();

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

    let driver = DriverService::new(DriverServiceConfig {
        listen_addr: driver_addr,
        worker_addrs: vec![w1_addr, w2_addr],
        master_addr: None,
    });
    let driver_task = tokio::spawn(async move { driver.run().await });

    let data_dir = tempfile::tempdir().unwrap();
    let vertices_path = data_dir.path().join("v.csv");
    let edges_path = data_dir.path().join("e.csv");
    std::fs::write(&vertices_path, "1\n2\n3\n").unwrap();
    std::fs::write(&edges_path, "1,2,0\n2,1,0\n2,3,0\n3,2,0\n").unwrap();

    let job = JobSpec {
        job_id: "job_test".to_string(),
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
        algorithm: AlgorithmSpec::Wcc { iterations: 10 },
        checkpoint: CheckpointSpec {
            enabled: false,
            interval_iters: 0,
            base_dir: "/tmp/geaflow-checkpoints".to_string(),
        },
    };

    let stream = loop {
        match TcpStream::connect(driver_addr).await {
            Ok(s) => break s,
            Err(_) => sleep(Duration::from_millis(20)).await,
        }
    };
    let mut framed = framed(stream);
    send_msg(
        &mut framed,
        &ClientToDriver::SubmitJob {
            job_spec: enc(&job),
        },
    )
    .await
    .unwrap();

    let resp: DriverToClient = recv_msg(&mut framed).await.unwrap();
    match resp {
        DriverToClient::JobAccepted { job_id } => assert_eq!(job_id, job.job_id),
        other => panic!("unexpected response: {other:?}"),
    }

    loop {
        send_msg(
            &mut framed,
            &ClientToDriver::GetJobStatus {
                job_id: job.job_id.clone(),
            },
        )
        .await
        .unwrap();
        let resp: DriverToClient = recv_msg(&mut framed).await.unwrap();
        match resp {
            DriverToClient::JobStatus { state, .. } => {
                if state == "finished" {
                    break;
                }
                if state == "failed" || state == "not_found" {
                    panic!("job state: {state}");
                }
            }
            DriverToClient::Error { message } => panic!("driver error: {message}"),
            other => panic!("unexpected response: {other:?}"),
        }
        sleep(Duration::from_millis(50)).await;
    }

    send_msg(
        &mut framed,
        &ClientToDriver::FetchVertices {
            job_id: job.job_id.clone(),
        },
    )
    .await
    .unwrap();
    let resp: DriverToClient = recv_msg(&mut framed).await.unwrap();
    let vertices = match resp {
        DriverToClient::Vertices { vertices, .. } => vertices,
        other => panic!("unexpected response: {other:?}"),
    };

    let mut decoded: Vec<(u64, u64)> = vertices
        .into_iter()
        .map(|(id, value)| (dec::<u64>(&id), dec::<u64>(&value)))
        .collect();
    decoded.sort_by_key(|(id, _)| *id);
    assert_eq!(decoded, vec![(1, 1), (2, 1), (3, 1)]);

    w1.abort();
    w2.abort();
    driver_task.abort();
}
