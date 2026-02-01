use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpec {
    pub job_id: String,
    pub name: String,
    pub mode: JobMode,
    pub graph: GraphSpec,
    pub algorithm: AlgorithmSpec,
    pub checkpoint: CheckpointSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobMode {
    Local,
    Distributed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSpec {
    pub vertices: FileSource,
    pub edges: FileSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileSource {
    Csv { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlgorithmSpec {
    Wcc { iterations: u64 },
    PageRank { iterations: u64, alpha: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointSpec {
    pub enabled: bool,
    pub interval_iters: u64,
    pub base_dir: String,
}
