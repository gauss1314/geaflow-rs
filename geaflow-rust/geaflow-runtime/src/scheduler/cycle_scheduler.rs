use crate::distributed::driver::DistributedDriver;
use crate::plan::job_spec::JobSpec;
use crate::state::checkpoint_meta::CheckpointMeta;
use geaflow_common::error::GeaFlowResult;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SchedulerResult {
    pub executed_iterations: u64,
}

#[derive(Debug, Clone)]
enum State {
    Init,
    Running { iteration: u64 },
    Finished { executed: u64 },
}

pub struct CycleScheduler;

impl CycleScheduler {
    pub async fn run(
        driver: &mut DistributedDriver,
        job: &JobSpec,
    ) -> GeaFlowResult<SchedulerResult> {
        let max_iterations = match &job.algorithm {
            crate::plan::job_spec::AlgorithmSpec::Wcc { iterations } => *iterations,
            crate::plan::job_spec::AlgorithmSpec::PageRank { iterations, .. } => *iterations,
        };

        let mut state = State::Init;
        let mut inboxes = DistributedDriver::new_inboxes(driver.worker_count());
        let mut start_iteration: u64 = 1;

        if job.checkpoint.enabled && job.checkpoint.interval_iters > 0 {
            let latest_path = CheckpointMeta::latest_path(&job.checkpoint.base_dir, &job.job_id);
            if latest_path.exists() {
                let meta = CheckpointMeta::read_json(&latest_path)?;
                driver
                    .load_checkpoint_all(Path::new(&meta.checkpoint_dir))
                    .await?;
                let bytes = std::fs::read(&meta.inboxes_path)
                    .map_err(geaflow_common::error::GeaFlowError::Io)?;
                inboxes = bincode::deserialize(&bytes)
                    .map_err(|e| geaflow_common::error::GeaFlowError::Internal(format!("{e}")))?;
                start_iteration = meta.iteration + 1;
            }
        }

        loop {
            state = match state {
                State::Init => State::Running {
                    iteration: start_iteration,
                },
                State::Running { iteration } => {
                    if iteration > max_iterations {
                        State::Finished {
                            executed: iteration - 1,
                        }
                    } else {
                        let (next_inboxes, any_msg) =
                            driver.superstep_round(iteration, &mut inboxes).await?;
                        inboxes = next_inboxes;
                        if job.checkpoint.enabled
                            && job.checkpoint.interval_iters > 0
                            && iteration % job.checkpoint.interval_iters == 0
                        {
                            let checkpoint_id = format!("{iteration}");
                            let checkpoint_dir = Path::new(&job.checkpoint.base_dir)
                                .join(&job.job_id)
                                .join(format!("cp_{checkpoint_id}"));
                            std::fs::create_dir_all(&checkpoint_dir)
                                .map_err(geaflow_common::error::GeaFlowError::Io)?;
                            driver.create_checkpoint_all(&checkpoint_dir).await?;

                            let inboxes_path = checkpoint_dir.join("inboxes.bin");
                            if let Some(parent) = inboxes_path.parent() {
                                std::fs::create_dir_all(parent)
                                    .map_err(geaflow_common::error::GeaFlowError::Io)?;
                            }
                            let bytes = bincode::serialize(&inboxes).map_err(|e| {
                                geaflow_common::error::GeaFlowError::Internal(format!("{e}"))
                            })?;
                            std::fs::write(&inboxes_path, bytes)
                                .map_err(geaflow_common::error::GeaFlowError::Io)?;

                            let meta = CheckpointMeta {
                                checkpoint_id: checkpoint_id.clone(),
                                iteration,
                                checkpoint_dir: checkpoint_dir.to_string_lossy().to_string(),
                                inboxes_path: inboxes_path.to_string_lossy().to_string(),
                            };
                            meta.write_json(CheckpointMeta::meta_path(
                                &job.checkpoint.base_dir,
                                &job.job_id,
                                &checkpoint_id,
                            ))?;
                            meta.write_json(CheckpointMeta::latest_path(
                                &job.checkpoint.base_dir,
                                &job.job_id,
                            ))?;
                        }
                        if any_msg {
                            State::Running {
                                iteration: iteration + 1,
                            }
                        } else {
                            State::Finished {
                                executed: iteration,
                            }
                        }
                    }
                }
                State::Finished { executed } => {
                    return Ok(SchedulerResult {
                        executed_iterations: executed,
                    })
                }
            };
        }
    }
}
