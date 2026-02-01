use bytes::Bytes;
use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use futures::{SinkExt, StreamExt};

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientToDriver {
    SubmitJob { job_spec: Vec<u8> },
    GetJobStatus { job_id: String },
    FetchVertices { job_id: String },
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum DriverToClient {
    JobAccepted {
        job_id: String,
    },
    JobStatus {
        job_id: String,
        state: String,
    },
    Vertices {
        job_id: String,
        vertices: Vec<(Vec<u8>, Vec<u8>)>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerToMaster {
    Register { worker_addr: String },
    Heartbeat { worker_addr: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum DriverToMaster {
    GetWorkers,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MasterRequest {
    Worker(WorkerToMaster),
    Driver(DriverToMaster),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MasterResponse {
    Ack,
    Workers { worker_addrs: Vec<String> },
    Error { message: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum DriverToWorker {
    LoadGraph {
        vertices: Vec<(Vec<u8>, Vec<u8>)>,
        edges: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>,
    },
    LoadGraphBatch {
        vertices: Vec<(Vec<u8>, Vec<u8>)>,
        edges: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>,
        last: bool,
    },
    SetAlgorithm {
        name: String,
        iterations: u64,
        params: Vec<u8>,
    },
    Superstep {
        iteration: u64,
        inbox: Vec<(Vec<u8>, Vec<Vec<u8>>)>,
    },
    SuperstepBatch {
        iteration: u64,
        inbox: Vec<(Vec<u8>, Vec<Vec<u8>>)>,
        last: bool,
    },
    CreateCheckpoint {
        checkpoint_dir: String,
    },
    LoadCheckpoint {
        checkpoint_dir: String,
    },
    FetchVertices,
    DumpVerticesCsv {
        output_path: String,
    },
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerToDriver {
    Ready,
    GraphLoaded {
        last: bool,
    },
    SuperstepResult {
        iteration: u64,
        outbox: Vec<(Vec<u8>, Vec<u8>)>,
    },
    SuperstepResultBatch {
        iteration: u64,
        outbox: Vec<(Vec<u8>, Vec<u8>)>,
        last: bool,
    },
    CheckpointCreated,
    CheckpointLoaded,
    Vertices {
        vertices: Vec<(Vec<u8>, Vec<u8>)>,
    },
    VerticesDumped {
        output_path: String,
    },
    Error {
        message: String,
    },
}

pub type DriverFramed = Framed<TcpStream, LengthDelimitedCodec>;

pub fn framed(stream: TcpStream) -> DriverFramed {
    Framed::new(stream, LengthDelimitedCodec::new())
}

pub async fn send_msg<T: Serialize>(framed: &mut DriverFramed, msg: &T) -> GeaFlowResult<()> {
    let bytes = bincode::serialize(msg)
        .map_err(|e| GeaFlowError::Internal(format!("bincode encode: {e}")))?;
    framed
        .send(Bytes::from(bytes))
        .await
        .map_err(|e| GeaFlowError::Internal(format!("send failed: {e}")))?;
    Ok(())
}

pub async fn recv_msg<T: for<'de> Deserialize<'de>>(framed: &mut DriverFramed) -> GeaFlowResult<T> {
    let bytes = framed
        .next()
        .await
        .ok_or_else(|| GeaFlowError::Internal("connection closed".to_string()))?
        .map_err(|e| GeaFlowError::Internal(format!("recv failed: {e}")))?;
    bincode::deserialize::<T>(&bytes)
        .map_err(|e| GeaFlowError::Internal(format!("bincode decode: {e}")))
}
