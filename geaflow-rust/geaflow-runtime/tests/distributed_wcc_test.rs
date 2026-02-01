use geaflow_runtime::distributed::driver::DistributedDriver;
use geaflow_runtime::distributed::worker::{run_worker, WorkerConfig};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

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
async fn test_distributed_wcc_end_to_end() {
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

    let mut driver = DistributedDriver::connect(&[w1_addr, w2_addr])
        .await
        .unwrap();

    let vertices: Vec<(Vec<u8>, Vec<u8>)> = vec![1u64, 2, 3]
        .into_iter()
        .map(|id| (enc(&id), enc(&id)))
        .collect();

    let edges: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> =
        vec![(1u64, 2u64), (2u64, 1u64), (2u64, 3u64), (3u64, 2u64)]
            .into_iter()
            .map(|(s, t)| (enc(&s), enc(&t), enc(&0u8)))
            .collect();

    driver.load_graph(vertices, edges).await.unwrap();
    driver
        .set_algorithm("wcc".to_string(), 10, Vec::new())
        .await
        .unwrap();
    driver.execute(10).await.unwrap();

    let mut result = driver.fetch_vertices().await.unwrap();
    driver.shutdown().await.unwrap();

    let _ = w1.await;
    let _ = w2.await;

    result.sort_by(|a, b| a.0.cmp(&b.0));

    let mut decoded: Vec<(u64, u64)> = result
        .into_iter()
        .map(|(id, value)| (dec::<u64>(&id), dec::<u64>(&value)))
        .collect();
    decoded.sort_by_key(|(id, _)| *id);

    assert_eq!(decoded, vec![(1, 1), (2, 1), (3, 1)]);
}
