use crate::distributed::algorithm::{DistributedAlgorithm, PageRankAlgorithm, WccAlgorithm};
use crate::distributed::protocol::{
    framed, recv_msg, send_msg, DriverToWorker, MasterRequest, WorkerToDriver, WorkerToMaster,
};
use crate::state::rocksdb_graph_state::RocksDbGraphState;
use crate::state::GraphState;
use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use geaflow_common::types::{Edge, Vertex};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};

pub struct WorkerConfig {
    pub listen_addr: SocketAddr,
    pub state_dir: PathBuf,
    pub master_addr: Option<SocketAddr>,
}

pub async fn run_worker(config: WorkerConfig) -> GeaFlowResult<()> {
    if let Some(master_addr) = config.master_addr {
        let worker_addr = config.listen_addr;
        tokio::spawn(async move {
            let _ = register_and_heartbeat(master_addr, worker_addr).await;
        });
    }

    let listener = TcpListener::bind(config.listen_addr)
        .await
        .map_err(|e| GeaFlowError::Internal(format!("bind worker: {e}")))?;
    let (stream, _) = listener
        .accept()
        .await
        .map_err(|e| GeaFlowError::Internal(format!("accept: {e}")))?;

    let mut framed = framed(stream);
    send_msg(&mut framed, &WorkerToDriver::Ready).await?;

    let mut state = RocksDbGraphState::open(&config.state_dir)?;

    let mut algorithm: Option<Box<dyn DistributedAlgorithm>> = None;
    let mut pending_iteration: Option<u64> = None;
    let mut pending_inbox: HashMap<Vec<u8>, Vec<Vec<u8>>> = HashMap::new();

    loop {
        let msg: DriverToWorker = recv_msg(&mut framed).await?;
        match msg {
            DriverToWorker::LoadGraph { vertices, edges } => {
                let v: Vec<Vertex<Vec<u8>, Vec<u8>>> = vertices
                    .into_iter()
                    .map(|(id, value)| Vertex { id, value })
                    .collect();
                <RocksDbGraphState as GraphState<Vec<u8>, Vec<u8>, Vec<u8>>>::put_vertex_batch(
                    &state, &v,
                )?;

                let e: Vec<Edge<Vec<u8>, Vec<u8>>> = edges
                    .into_iter()
                    .map(|(src, target, value)| Edge {
                        src_id: src,
                        target_id: target,
                        value,
                    })
                    .collect();
                <RocksDbGraphState as GraphState<Vec<u8>, Vec<u8>, Vec<u8>>>::put_edge_batch(
                    &state, &e,
                )?;
            }
            DriverToWorker::LoadGraphBatch {
                vertices,
                edges,
                last,
            } => {
                if !vertices.is_empty() {
                    let v: Vec<Vertex<Vec<u8>, Vec<u8>>> = vertices
                        .into_iter()
                        .map(|(id, value)| Vertex { id, value })
                        .collect();
                    <RocksDbGraphState as GraphState<Vec<u8>, Vec<u8>, Vec<u8>>>::put_vertex_batch(
                        &state, &v,
                    )?;
                }

                if !edges.is_empty() {
                    let e: Vec<Edge<Vec<u8>, Vec<u8>>> = edges
                        .into_iter()
                        .map(|(src, target, value)| Edge {
                            src_id: src,
                            target_id: target,
                            value,
                        })
                        .collect();
                    <RocksDbGraphState as GraphState<Vec<u8>, Vec<u8>, Vec<u8>>>::put_edge_batch(
                        &state, &e,
                    )?;
                }

                send_msg(&mut framed, &WorkerToDriver::GraphLoaded { last }).await?;
            }
            DriverToWorker::SetAlgorithm {
                name,
                iterations,
                params,
            } => {
                let algo: Box<dyn DistributedAlgorithm> = match name.as_str() {
                    "wcc" => Box::new(WccAlgorithm::new(iterations)),
                    "pagerank" => Box::new(PageRankAlgorithm::from_params(iterations, &params)?),
                    other => {
                        return Err(GeaFlowError::InvalidArgument(format!(
                            "unknown algorithm: {other}"
                        )))
                    }
                };
                algorithm = Some(algo);
            }
            DriverToWorker::Superstep { iteration, inbox } => {
                pending_inbox.clear();
                for (k, msgs) in inbox {
                    pending_inbox.insert(k, msgs);
                }
                let mut inbox_map = std::mem::take(&mut pending_inbox);
                let algo = algorithm.as_mut().ok_or_else(|| {
                    GeaFlowError::InvalidArgument("algorithm not set".to_string())
                })?;
                process_superstep(iteration, &mut inbox_map, &state, algo, &mut framed).await?;
            }
            DriverToWorker::SuperstepBatch {
                iteration,
                inbox,
                last,
            } => {
                if pending_iteration != Some(iteration) {
                    pending_iteration = Some(iteration);
                    pending_inbox.clear();
                }
                for (k, mut msgs) in inbox {
                    pending_inbox.entry(k).or_default().append(&mut msgs);
                }
                if last {
                    let mut inbox_map = std::mem::take(&mut pending_inbox);
                    let algo = algorithm.as_mut().ok_or_else(|| {
                        GeaFlowError::InvalidArgument("algorithm not set".to_string())
                    })?;
                    process_superstep(iteration, &mut inbox_map, &state, algo, &mut framed).await?;
                    pending_iteration = None;
                }
            }
            DriverToWorker::CreateCheckpoint { checkpoint_dir } => {
                let checkpoint_dir = Path::new(&checkpoint_dir);
                match state.create_checkpoint(checkpoint_dir) {
                    Ok(_) => {
                        send_msg(&mut framed, &WorkerToDriver::CheckpointCreated).await?;
                    }
                    Err(e) => {
                        send_msg(
                            &mut framed,
                            &WorkerToDriver::Error {
                                message: format!("{e}"),
                            },
                        )
                        .await?;
                    }
                }
            }
            DriverToWorker::LoadCheckpoint { checkpoint_dir } => {
                match RocksDbGraphState::open(Path::new(&checkpoint_dir)) {
                    Ok(s) => {
                        state = s;
                        send_msg(&mut framed, &WorkerToDriver::CheckpointLoaded).await?;
                    }
                    Err(e) => {
                        send_msg(
                            &mut framed,
                            &WorkerToDriver::Error {
                                message: format!("{e}"),
                            },
                        )
                        .await?;
                    }
                }
            }
            DriverToWorker::FetchVertices => {
                let vertices =
                    <RocksDbGraphState as GraphState<Vec<u8>, Vec<u8>, Vec<u8>>>::list_vertices(
                        &state,
                    )?;
                let vertices: Vec<(Vec<u8>, Vec<u8>)> =
                    vertices.into_iter().map(|v| (v.id, v.value)).collect();
                send_msg(&mut framed, &WorkerToDriver::Vertices { vertices }).await?;
            }
            DriverToWorker::DumpVerticesCsv { output_path } => {
                let algo_name = algorithm.as_ref().map(|a| a.name()).unwrap_or("unknown");
                let dump_result = match algo_name {
                    "wcc" => state.dump_vertices_csv_u64_u64(Path::new(&output_path)),
                    "pagerank" => state.dump_vertices_csv_u64_f64(Path::new(&output_path)),
                    other => Err(GeaFlowError::InvalidArgument(format!(
                        "unsupported algorithm for dump: {other}"
                    ))),
                };

                if let Err(e) = dump_result {
                    send_msg(
                        &mut framed,
                        &WorkerToDriver::Error {
                            message: format!("{e}"),
                        },
                    )
                    .await?;
                } else {
                    send_msg(&mut framed, &WorkerToDriver::VerticesDumped { output_path }).await?;
                }
            }
            DriverToWorker::Shutdown => {
                break;
            }
        }
    }

    Ok(())
}

async fn process_superstep(
    iteration: u64,
    inbox_map: &mut HashMap<Vec<u8>, Vec<Vec<u8>>>,
    state: &RocksDbGraphState,
    algo: &mut Box<dyn DistributedAlgorithm>,
    framed: &mut crate::distributed::protocol::DriverFramed,
) -> GeaFlowResult<()> {
    let start = std::time::Instant::now();

    let vertices =
        <RocksDbGraphState as GraphState<Vec<u8>, Vec<u8>, Vec<u8>>>::list_vertices(state)?;

    let mut updates: Vec<Vertex<Vec<u8>, Vec<u8>>> = Vec::new();
    let mut outbox: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();

    for v in vertices {
        let msgs = inbox_map.remove(&v.id).unwrap_or_default();
        let out_edges =
            <RocksDbGraphState as GraphState<Vec<u8>, Vec<u8>, Vec<u8>>>::get_out_edges(
                state, &v.id,
            )?;
        let out_edges: Vec<(Vec<u8>, Vec<u8>)> = out_edges
            .into_iter()
            .map(|e| (e.target_id, e.value))
            .collect();

        let (new_value, mut outgoing) =
            algo.compute_vertex(&v.id, Some(&v.value), &out_edges, &msgs, iteration)?;

        if let Some(nv) = new_value {
            updates.push(Vertex {
                id: v.id.clone(),
                value: nv,
            });
        }
        outbox.append(&mut outgoing);
    }

    if !updates.is_empty() {
        <RocksDbGraphState as GraphState<Vec<u8>, Vec<u8>, Vec<u8>>>::put_vertex_batch(
            state, &updates,
        )?;
    }

    metrics::counter!("geaflow_worker_superstep_updates_total").increment(updates.len() as u64);
    metrics::counter!("geaflow_worker_superstep_outbox_total").increment(outbox.len() as u64);
    metrics::histogram!("geaflow_worker_superstep_duration_ms")
        .record(start.elapsed().as_secs_f64() * 1000.0);

    let batch_entries: usize = 256;
    if outbox.is_empty() {
        send_msg(
            framed,
            &WorkerToDriver::SuperstepResultBatch {
                iteration,
                outbox: Vec::new(),
                last: true,
            },
        )
        .await?;
        return Ok(());
    }

    let mut offset = 0;
    while offset < outbox.len() {
        let end = (offset + batch_entries).min(outbox.len());
        let last = end == outbox.len();
        send_msg(
            framed,
            &WorkerToDriver::SuperstepResultBatch {
                iteration,
                outbox: outbox[offset..end].to_vec(),
                last,
            },
        )
        .await?;
        offset = end;
    }
    Ok(())
}

async fn register_and_heartbeat(
    master_addr: SocketAddr,
    worker_addr: SocketAddr,
) -> GeaFlowResult<()> {
    let worker_addr = worker_addr.to_string();
    loop {
        match tokio::net::TcpStream::connect(master_addr).await {
            Ok(stream) => {
                let mut framed = framed(stream);
                send_msg(
                    &mut framed,
                    &MasterRequest::Worker(WorkerToMaster::Register {
                        worker_addr: worker_addr.clone(),
                    }),
                )
                .await?;
                break;
            }
            Err(_) => sleep(Duration::from_millis(200)).await,
        }
    }

    loop {
        if let Ok(stream) = tokio::net::TcpStream::connect(master_addr).await {
            let mut framed = framed(stream);
            let _ = send_msg(
                &mut framed,
                &MasterRequest::Worker(WorkerToMaster::Heartbeat {
                    worker_addr: worker_addr.clone(),
                }),
            )
            .await;
        }
        sleep(Duration::from_secs(1)).await;
    }
}
