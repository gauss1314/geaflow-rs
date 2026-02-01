use crate::distributed::protocol::{
    framed, recv_msg, send_msg, DriverFramed, DriverToWorker, WorkerToDriver,
};
use crate::shuffle::MessageShuffle;
use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use std::collections::HashMap;
use std::io::BufRead;
use std::net::SocketAddr;
use std::path::Path;
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration};

pub type Inbox = HashMap<Vec<u8>, Vec<Vec<u8>>>;
pub type Inboxes = Vec<Inbox>;
type EdgeBytes = (Vec<u8>, Vec<u8>, Vec<u8>);

pub struct DistributedDriver {
    workers: Vec<DriverFramed>,
}

impl DistributedDriver {
    pub async fn run_job(
        worker_addrs: &[SocketAddr],
        job: &crate::plan::job_spec::JobSpec,
    ) -> GeaFlowResult<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut driver = Self::connect(worker_addrs).await?;

        let edges = match &job.graph.edges {
            crate::plan::job_spec::FileSource::Csv { path } => {
                crate::io::file::read_edges_u64_u8(path, 0)?
            }
        };

        match &job.algorithm {
            crate::plan::job_spec::AlgorithmSpec::Wcc { iterations } => {
                let vertices = match &job.graph.vertices {
                    crate::plan::job_spec::FileSource::Csv { path } => {
                        crate::io::file::read_vertices_u64_u64_id_default(path)?
                    }
                };

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

                driver.load_graph(vertices, edges).await?;
                driver
                    .set_algorithm("wcc".to_string(), *iterations, Vec::new())
                    .await?;
                crate::scheduler::cycle_scheduler::CycleScheduler::run(&mut driver, job).await?;
            }
            crate::plan::job_spec::AlgorithmSpec::PageRank { iterations, alpha } => {
                let vertices = match &job.graph.vertices {
                    crate::plan::job_spec::FileSource::Csv { path } => {
                        crate::io::file::read_vertices_u64_f64(path, 1.0)?
                    }
                };

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

                driver.load_graph(vertices, edges).await?;
                let params = crate::distributed::algorithm::PageRankParams { alpha: *alpha };
                driver
                    .set_algorithm(
                        "pagerank".to_string(),
                        *iterations,
                        bincode::serialize(&params).unwrap(),
                    )
                    .await?;
                crate::scheduler::cycle_scheduler::CycleScheduler::run(&mut driver, job).await?;
            }
        }

        let vertices = driver.fetch_vertices().await?;
        driver.shutdown().await?;
        Ok(vertices)
    }

    pub async fn connect(worker_addrs: &[SocketAddr]) -> GeaFlowResult<Self> {
        let mut workers = Vec::with_capacity(worker_addrs.len());
        for addr in worker_addrs {
            let mut attempts: u32 = 0;
            let stream = loop {
                attempts += 1;
                match TcpStream::connect(addr).await {
                    Ok(s) => break s,
                    Err(e) => {
                        if attempts >= 200 {
                            return Err(GeaFlowError::Internal(format!(
                                "connect {addr} failed: {}",
                                e
                            )));
                        }
                        sleep(Duration::from_millis(30)).await;
                    }
                }
            };
            let mut framed = framed(stream);
            let ready: WorkerToDriver = recv_msg(&mut framed).await?;
            match ready {
                WorkerToDriver::Ready => {}
                other => {
                    return Err(GeaFlowError::Internal(format!(
                        "unexpected handshake from worker {addr:?}: {other:?}"
                    )))
                }
            }
            workers.push(framed);
        }
        Ok(Self { workers })
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub fn new_inboxes(worker_count: usize) -> Inboxes {
        (0..worker_count).map(|_| HashMap::new()).collect()
    }

    pub async fn superstep_round(
        &mut self,
        iteration: u64,
        inboxes: &mut Inboxes,
    ) -> GeaFlowResult<(Inboxes, bool)> {
        let n = self.worker_count().max(1);
        let batch_entries: usize = 256;
        for (worker, inbox) in self.workers.iter_mut().zip(inboxes.iter_mut()) {
            let inbox_vec: Vec<(Vec<u8>, Vec<Vec<u8>>)> = inbox.drain().collect();
            if inbox_vec.is_empty() {
                send_msg(
                    worker,
                    &DriverToWorker::SuperstepBatch {
                        iteration,
                        inbox: Vec::new(),
                        last: true,
                    },
                )
                .await?;
            } else {
                let mut offset = 0;
                while offset < inbox_vec.len() {
                    let end = (offset + batch_entries).min(inbox_vec.len());
                    let last = end == inbox_vec.len();
                    send_msg(
                        worker,
                        &DriverToWorker::SuperstepBatch {
                            iteration,
                            inbox: inbox_vec[offset..end].to_vec(),
                            last,
                        },
                    )
                    .await?;
                    offset = end;
                }
            }
        }

        let mut next_inboxes: Inboxes = (0..n).map(|_| HashMap::new()).collect();
        let mut any_msg = false;

        for worker in &mut self.workers {
            loop {
                let resp: WorkerToDriver = recv_msg(worker).await?;
                match resp {
                    WorkerToDriver::SuperstepResult { outbox, .. } => {
                        if !outbox.is_empty() {
                            any_msg = true;
                        }
                        let shuffler = crate::shuffle::DriverShuffle;
                        shuffler.route_outbox(outbox, n, &mut next_inboxes);
                        break;
                    }
                    WorkerToDriver::SuperstepResultBatch { outbox, last, .. } => {
                        if !outbox.is_empty() {
                            any_msg = true;
                        }
                        let shuffler = crate::shuffle::DriverShuffle;
                        shuffler.route_outbox(outbox, n, &mut next_inboxes);
                        if last {
                            break;
                        }
                    }
                    WorkerToDriver::Error { message } => {
                        return Err(GeaFlowError::Internal(format!("worker error: {message}")))
                    }
                    other => {
                        return Err(GeaFlowError::Internal(format!(
                            "unexpected worker response: {other:?}"
                        )))
                    }
                }
            }
        }

        Ok((next_inboxes, any_msg))
    }

    pub async fn load_graph(
        &mut self,
        vertices: Vec<(Vec<u8>, Vec<u8>)>,
        edges: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>,
    ) -> GeaFlowResult<()> {
        let n = self.worker_count().max(1);
        let mut v_parts: Vec<Vec<(Vec<u8>, Vec<u8>)>> = (0..n).map(|_| Vec::new()).collect();
        for (id, value) in vertices {
            let p = partition_of(&id, n);
            v_parts[p].push((id, value));
        }

        let mut e_parts: Vec<Vec<EdgeBytes>> = (0..n).map(|_| Vec::new()).collect();
        for (src, target, value) in edges {
            let p = partition_of(&src, n);
            e_parts[p].push((src, target, value));
        }

        for (i, worker) in self.workers.iter_mut().enumerate() {
            send_msg(
                worker,
                &DriverToWorker::LoadGraph {
                    vertices: std::mem::take(&mut v_parts[i]),
                    edges: std::mem::take(&mut e_parts[i]),
                },
            )
            .await?;
        }
        Ok(())
    }

    pub async fn load_graph_batch(
        &mut self,
        worker_index: usize,
        vertices: Vec<(Vec<u8>, Vec<u8>)>,
        edges: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>,
        last: bool,
    ) -> GeaFlowResult<()> {
        let worker = self
            .workers
            .get_mut(worker_index)
            .ok_or_else(|| GeaFlowError::InvalidArgument("bad worker index".to_string()))?;
        send_msg(
            worker,
            &DriverToWorker::LoadGraphBatch {
                vertices,
                edges,
                last,
            },
        )
        .await?;
        let ack: WorkerToDriver = recv_msg(worker).await?;
        match ack {
            WorkerToDriver::GraphLoaded { .. } => Ok(()),
            WorkerToDriver::Error { message } => {
                Err(GeaFlowError::Internal(format!("worker error: {message}")))
            }
            other => Err(GeaFlowError::Internal(format!(
                "unexpected graph load ack: {other:?}"
            ))),
        }
    }

    pub async fn load_graph500_streaming<F>(
        &mut self,
        vertices_path: impl AsRef<Path>,
        edges_path: impl AsRef<Path>,
        mut vertex_value: F,
        vertex_batch_size: usize,
        edge_batch_size: usize,
        undirected: bool,
    ) -> GeaFlowResult<()>
    where
        F: FnMut(u64) -> Vec<u8>,
    {
        let n = self.worker_count().max(1);
        let mut v_bufs: Vec<Vec<(Vec<u8>, Vec<u8>)>> = (0..n).map(|_| Vec::new()).collect();
        let mut e_bufs: Vec<Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>> =
            (0..n).map(|_| Vec::new()).collect();

        let vertices_f = std::fs::File::open(vertices_path.as_ref())
            .map_err(|e| GeaFlowError::Internal(format!("open vertices: {e}")))?;
        let vertices_r = std::io::BufReader::new(vertices_f);
        for line in vertices_r.lines() {
            let line = line.map_err(|e| GeaFlowError::Internal(format!("read vertices: {e}")))?;
            let s = line.trim();
            if s.is_empty() || s.starts_with('#') {
                continue;
            }
            let mut it = s.split_whitespace();
            let id: u64 = it
                .next()
                .ok_or_else(|| GeaFlowError::InvalidArgument("vertex id missing".to_string()))?
                .parse()
                .map_err(|e| GeaFlowError::InvalidArgument(format!("vertex id parse: {e}")))?;
            let id_bytes = bincode::serialize(&id)
                .map_err(|e| GeaFlowError::Internal(format!("encode vertex id: {e}")))?;
            let value_bytes = vertex_value(id);
            let p = partition_of(&id_bytes, n);
            v_bufs[p].push((id_bytes, value_bytes));
            if v_bufs[p].len() >= vertex_batch_size.max(1) {
                self.load_graph_batch(p, std::mem::take(&mut v_bufs[p]), Vec::new(), false)
                    .await?;
            }
        }

        for p in 0..n {
            if !v_bufs[p].is_empty() {
                self.load_graph_batch(p, std::mem::take(&mut v_bufs[p]), Vec::new(), false)
                    .await?;
            }
        }

        let edges_f = std::fs::File::open(edges_path.as_ref())
            .map_err(|e| GeaFlowError::Internal(format!("open edges: {e}")))?;
        let edges_r = std::io::BufReader::new(edges_f);
        let edge_value_bytes = bincode::serialize(&0u8)
            .map_err(|e| GeaFlowError::Internal(format!("encode edge value: {e}")))?;
        for line in edges_r.lines() {
            let line = line.map_err(|e| GeaFlowError::Internal(format!("read edges: {e}")))?;
            let s = line.trim();
            if s.is_empty() || s.starts_with('#') {
                continue;
            }
            let mut it = s.split_whitespace();
            let src: u64 = it
                .next()
                .ok_or_else(|| GeaFlowError::InvalidArgument("edge src missing".to_string()))?
                .parse()
                .map_err(|e| GeaFlowError::InvalidArgument(format!("edge src parse: {e}")))?;
            let dst: u64 = it
                .next()
                .ok_or_else(|| GeaFlowError::InvalidArgument("edge dst missing".to_string()))?
                .parse()
                .map_err(|e| GeaFlowError::InvalidArgument(format!("edge dst parse: {e}")))?;
            let src_bytes = bincode::serialize(&src)
                .map_err(|e| GeaFlowError::Internal(format!("encode edge src: {e}")))?;
            let dst_bytes = bincode::serialize(&dst)
                .map_err(|e| GeaFlowError::Internal(format!("encode edge dst: {e}")))?;
            {
                let p = partition_of(&src_bytes, n);
                e_bufs[p].push((
                    src_bytes.clone(),
                    dst_bytes.clone(),
                    edge_value_bytes.clone(),
                ));
                if e_bufs[p].len() >= edge_batch_size.max(1) {
                    self.load_graph_batch(p, Vec::new(), std::mem::take(&mut e_bufs[p]), false)
                        .await?;
                }
            }
            if undirected {
                let p = partition_of(&dst_bytes, n);
                e_bufs[p].push((dst_bytes, src_bytes, edge_value_bytes.clone()));
                if e_bufs[p].len() >= edge_batch_size.max(1) {
                    self.load_graph_batch(p, Vec::new(), std::mem::take(&mut e_bufs[p]), false)
                        .await?;
                }
            }
        }

        for p in 0..n {
            if !e_bufs[p].is_empty() {
                self.load_graph_batch(p, Vec::new(), std::mem::take(&mut e_bufs[p]), false)
                    .await?;
            }
        }

        for p in 0..n {
            self.load_graph_batch(p, Vec::new(), Vec::new(), true)
                .await?;
        }
        Ok(())
    }

    pub async fn load_graph500_streaming_generated_vertices<F>(
        &mut self,
        vertex_count: u64,
        edges_path: impl AsRef<Path>,
        mut vertex_value: F,
        vertex_batch_size: usize,
        edge_batch_size: usize,
        undirected: bool,
    ) -> GeaFlowResult<()>
    where
        F: FnMut(u64) -> Vec<u8>,
    {
        let n = self.worker_count().max(1);
        let mut v_bufs: Vec<Vec<(Vec<u8>, Vec<u8>)>> = (0..n).map(|_| Vec::new()).collect();
        let mut e_bufs: Vec<Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>> =
            (0..n).map(|_| Vec::new()).collect();

        for id in 0..vertex_count {
            let id_bytes = bincode::serialize(&id)
                .map_err(|e| GeaFlowError::Internal(format!("encode vertex id: {e}")))?;
            let value_bytes = vertex_value(id);
            let p = partition_of(&id_bytes, n);
            v_bufs[p].push((id_bytes, value_bytes));
            if v_bufs[p].len() >= vertex_batch_size.max(1) {
                self.load_graph_batch(p, std::mem::take(&mut v_bufs[p]), Vec::new(), false)
                    .await?;
            }
        }

        for p in 0..n {
            if !v_bufs[p].is_empty() {
                self.load_graph_batch(p, std::mem::take(&mut v_bufs[p]), Vec::new(), false)
                    .await?;
            }
        }

        let edges_f = std::fs::File::open(edges_path.as_ref())
            .map_err(|e| GeaFlowError::Internal(format!("open edges: {e}")))?;
        let edges_r = std::io::BufReader::new(edges_f);
        let edge_value_bytes = bincode::serialize(&0u8)
            .map_err(|e| GeaFlowError::Internal(format!("encode edge value: {e}")))?;
        for line in edges_r.lines() {
            let line = line.map_err(|e| GeaFlowError::Internal(format!("read edges: {e}")))?;
            let s = line.trim();
            if s.is_empty() || s.starts_with('#') {
                continue;
            }
            let mut it = s.split_whitespace();
            let src: u64 = it
                .next()
                .ok_or_else(|| GeaFlowError::InvalidArgument("edge src missing".to_string()))?
                .parse()
                .map_err(|e| GeaFlowError::InvalidArgument(format!("edge src parse: {e}")))?;
            let dst: u64 = it
                .next()
                .ok_or_else(|| GeaFlowError::InvalidArgument("edge dst missing".to_string()))?
                .parse()
                .map_err(|e| GeaFlowError::InvalidArgument(format!("edge dst parse: {e}")))?;
            let src_bytes = bincode::serialize(&src)
                .map_err(|e| GeaFlowError::Internal(format!("encode edge src: {e}")))?;
            let dst_bytes = bincode::serialize(&dst)
                .map_err(|e| GeaFlowError::Internal(format!("encode edge dst: {e}")))?;

            {
                let p = partition_of(&src_bytes, n);
                e_bufs[p].push((
                    src_bytes.clone(),
                    dst_bytes.clone(),
                    edge_value_bytes.clone(),
                ));
                if e_bufs[p].len() >= edge_batch_size.max(1) {
                    self.load_graph_batch(p, Vec::new(), std::mem::take(&mut e_bufs[p]), false)
                        .await?;
                }
            }
            if undirected {
                let p = partition_of(&dst_bytes, n);
                e_bufs[p].push((dst_bytes, src_bytes, edge_value_bytes.clone()));
                if e_bufs[p].len() >= edge_batch_size.max(1) {
                    self.load_graph_batch(p, Vec::new(), std::mem::take(&mut e_bufs[p]), false)
                        .await?;
                }
            }
        }

        for p in 0..n {
            if !e_bufs[p].is_empty() {
                self.load_graph_batch(p, Vec::new(), std::mem::take(&mut e_bufs[p]), false)
                    .await?;
            }
        }

        for p in 0..n {
            self.load_graph_batch(p, Vec::new(), Vec::new(), true)
                .await?;
        }

        Ok(())
    }

    pub async fn dump_vertices_csv(
        &mut self,
        output_dir: impl AsRef<Path>,
        file_prefix: &str,
    ) -> GeaFlowResult<Vec<std::path::PathBuf>> {
        let output_dir = output_dir.as_ref();
        std::fs::create_dir_all(output_dir).map_err(GeaFlowError::Io)?;
        let mut out = Vec::with_capacity(self.workers.len());

        for (i, worker) in self.workers.iter_mut().enumerate() {
            let path = output_dir.join(format!("{file_prefix}_part_{i}.csv"));
            let path_s = path.to_string_lossy().to_string();
            send_msg(
                worker,
                &DriverToWorker::DumpVerticesCsv {
                    output_path: path_s.clone(),
                },
            )
            .await?;
            let resp: WorkerToDriver = recv_msg(worker).await?;
            match resp {
                WorkerToDriver::VerticesDumped { .. } => {
                    out.push(path);
                }
                WorkerToDriver::Error { message } => {
                    return Err(GeaFlowError::Internal(format!("worker error: {message}")));
                }
                other => {
                    return Err(GeaFlowError::Internal(format!(
                        "unexpected dump response: {other:?}"
                    )));
                }
            }
        }
        Ok(out)
    }

    pub async fn set_algorithm(
        &mut self,
        name: String,
        iterations: u64,
        params: Vec<u8>,
    ) -> GeaFlowResult<()> {
        for w in &mut self.workers {
            send_msg(
                w,
                &DriverToWorker::SetAlgorithm {
                    name: name.clone(),
                    iterations,
                    params: params.clone(),
                },
            )
            .await?;
        }
        Ok(())
    }

    pub async fn execute(&mut self, iterations: u64) -> GeaFlowResult<u64> {
        let n = self.worker_count().max(1);
        let mut inboxes: Inboxes = (0..n).map(|_| HashMap::new()).collect();

        let mut iteration: u64 = 1;
        while iteration <= iterations {
            let (next_inboxes, any_msg) = self.superstep_round(iteration, &mut inboxes).await?;
            iteration += 1;
            inboxes = next_inboxes;

            if !any_msg {
                break;
            }
        }

        Ok(iteration - 1)
    }

    pub async fn create_checkpoint_all(&mut self, base_dir: impl AsRef<Path>) -> GeaFlowResult<()> {
        let base_dir = base_dir.as_ref();
        for (i, w) in self.workers.iter_mut().enumerate() {
            let checkpoint_dir = base_dir.join(format!("worker_{i}"));
            send_msg(
                w,
                &DriverToWorker::CreateCheckpoint {
                    checkpoint_dir: checkpoint_dir.to_string_lossy().to_string(),
                },
            )
            .await?;
        }

        for w in &mut self.workers {
            let resp: WorkerToDriver = recv_msg(w).await?;
            match resp {
                WorkerToDriver::CheckpointCreated => {}
                WorkerToDriver::Error { message } => {
                    return Err(GeaFlowError::Internal(format!(
                        "checkpoint failed: {message}"
                    )))
                }
                other => {
                    return Err(GeaFlowError::Internal(format!(
                        "unexpected checkpoint response: {other:?}"
                    )))
                }
            }
        }
        Ok(())
    }

    pub async fn load_checkpoint_all(&mut self, base_dir: impl AsRef<Path>) -> GeaFlowResult<()> {
        let base_dir = base_dir.as_ref();
        for (i, w) in self.workers.iter_mut().enumerate() {
            let checkpoint_dir = base_dir.join(format!("worker_{i}"));
            send_msg(
                w,
                &DriverToWorker::LoadCheckpoint {
                    checkpoint_dir: checkpoint_dir.to_string_lossy().to_string(),
                },
            )
            .await?;
        }

        for w in &mut self.workers {
            let resp: WorkerToDriver = recv_msg(w).await?;
            match resp {
                WorkerToDriver::CheckpointLoaded => {}
                WorkerToDriver::Error { message } => {
                    return Err(GeaFlowError::Internal(format!(
                        "load checkpoint failed: {message}"
                    )))
                }
                other => {
                    return Err(GeaFlowError::Internal(format!(
                        "unexpected load checkpoint response: {other:?}"
                    )))
                }
            }
        }
        Ok(())
    }

    pub async fn fetch_vertices(&mut self) -> GeaFlowResult<Vec<(Vec<u8>, Vec<u8>)>> {
        for w in &mut self.workers {
            send_msg(w, &DriverToWorker::FetchVertices).await?;
        }

        let mut out = Vec::new();
        for w in &mut self.workers {
            let resp: WorkerToDriver = recv_msg(w).await?;
            match resp {
                WorkerToDriver::Vertices { mut vertices } => out.append(&mut vertices),
                WorkerToDriver::Error { message } => {
                    return Err(GeaFlowError::Internal(format!(
                        "fetch vertices failed: {message}"
                    )))
                }
                other => {
                    return Err(GeaFlowError::Internal(format!(
                        "unexpected fetch response: {other:?}"
                    )))
                }
            }
        }
        Ok(out)
    }

    pub async fn shutdown(mut self) -> GeaFlowResult<()> {
        for w in &mut self.workers {
            send_msg(w, &DriverToWorker::Shutdown).await?;
        }
        Ok(())
    }
}

fn partition_of(id: &[u8], partitions: usize) -> usize {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    let mut h = DefaultHasher::new();
    h.write(id);
    (h.finish() as usize) % partitions
}
