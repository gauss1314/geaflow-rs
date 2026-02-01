use crate::plan::job_spec::JobSpec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub job_id: String,
    pub worker_count: usize,
    pub partitions: usize,
    pub algorithm_name: String,
    pub max_iterations: u64,
}

impl ExecutionPlan {
    pub fn from_job_spec(job: &JobSpec, worker_count: usize, partitions: usize) -> Self {
        let (algorithm_name, max_iterations) = match &job.algorithm {
            crate::plan::job_spec::AlgorithmSpec::Wcc { iterations } => {
                ("wcc".to_string(), *iterations)
            }
            crate::plan::job_spec::AlgorithmSpec::PageRank { iterations, .. } => {
                ("pagerank".to_string(), *iterations)
            }
        };
        Self {
            job_id: job.job_id.clone(),
            worker_count,
            partitions,
            algorithm_name,
            max_iterations,
        }
    }
}
