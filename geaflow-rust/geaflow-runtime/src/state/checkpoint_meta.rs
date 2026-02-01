use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMeta {
    pub checkpoint_id: String,
    pub iteration: u64,
    pub checkpoint_dir: String,
    pub inboxes_path: String,
}

impl CheckpointMeta {
    pub fn meta_path(base_dir: impl AsRef<Path>, job_id: &str, checkpoint_id: &str) -> PathBuf {
        base_dir
            .as_ref()
            .join(job_id)
            .join(format!("checkpoint_{checkpoint_id}.json"))
    }

    pub fn latest_path(base_dir: impl AsRef<Path>, job_id: &str) -> PathBuf {
        base_dir
            .as_ref()
            .join(job_id)
            .join("checkpoint_latest.json")
    }

    pub fn write_json(&self, path: impl AsRef<Path>) -> GeaFlowResult<()> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(GeaFlowError::Io)?;
        }
        let s = serde_json::to_string_pretty(self)
            .map_err(|e| GeaFlowError::Internal(format!("{e}")))?;
        std::fs::write(path, s).map_err(GeaFlowError::Io)
    }

    pub fn read_json(path: impl AsRef<Path>) -> GeaFlowResult<Self> {
        let s = std::fs::read_to_string(path).map_err(GeaFlowError::Io)?;
        serde_json::from_str(&s).map_err(|e| GeaFlowError::Internal(format!("{e}")))
    }
}
