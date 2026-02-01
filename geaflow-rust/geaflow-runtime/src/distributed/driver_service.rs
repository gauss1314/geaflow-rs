use crate::distributed::protocol::{
    framed, recv_msg, send_msg, ClientToDriver, DriverToClient, DriverToMaster, MasterRequest,
    MasterResponse,
};
use crate::plan::job_spec::JobSpec;
use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct DriverServiceConfig {
    pub listen_addr: SocketAddr,
    pub worker_addrs: Vec<SocketAddr>,
    pub master_addr: Option<SocketAddr>,
}

#[derive(Clone)]
pub struct DriverService {
    config: DriverServiceConfig,
    jobs: Arc<Mutex<HashMap<String, JobEntry>>>,
}

#[derive(Clone)]
struct JobEntry {
    state: String,
    result_vertices: Option<Vec<(Vec<u8>, Vec<u8>)>>,
    error: Option<String>,
}

impl DriverService {
    pub fn new(config: DriverServiceConfig) -> Self {
        Self {
            config,
            jobs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn run(&self) -> GeaFlowResult<()> {
        let listener = TcpListener::bind(self.config.listen_addr)
            .await
            .map_err(|e| GeaFlowError::Internal(format!("bind driver: {e}")))?;

        loop {
            let (stream, _) = listener
                .accept()
                .await
                .map_err(|e| GeaFlowError::Internal(format!("accept: {e}")))?;
            let service = self.clone();
            tokio::spawn(async move {
                let _ = service.handle_connection(stream).await;
            });
        }
    }

    pub async fn list_jobs(&self) -> Vec<(String, String)> {
        let jobs = self.jobs.lock().await;
        let mut out: Vec<(String, String)> = jobs
            .iter()
            .map(|(id, j)| (id.clone(), j.state.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    async fn handle_connection(&self, stream: tokio::net::TcpStream) -> GeaFlowResult<()> {
        let mut framed = framed(stream);

        loop {
            let req: ClientToDriver = recv_msg(&mut framed).await?;
            match req {
                ClientToDriver::SubmitJob { job_spec } => {
                    let job: JobSpec = bincode::deserialize(&job_spec).map_err(|e| {
                        GeaFlowError::InvalidArgument(format!("invalid job_spec: {e}"))
                    })?;

                    {
                        let mut jobs = self.jobs.lock().await;
                        jobs.insert(
                            job.job_id.clone(),
                            JobEntry {
                                state: "running".to_string(),
                                result_vertices: None,
                                error: None,
                            },
                        );
                    }

                    let worker_addrs = match self.resolve_workers().await {
                        Ok(w) => w,
                        Err(e) => {
                            send_msg(
                                &mut framed,
                                &DriverToClient::Error {
                                    message: format!("{e}"),
                                },
                            )
                            .await?;
                            continue;
                        }
                    };

                    send_msg(
                        &mut framed,
                        &DriverToClient::JobAccepted {
                            job_id: job.job_id.clone(),
                        },
                    )
                    .await?;

                    let jobs = self.jobs.clone();
                    tokio::spawn(async move {
                        let result = crate::distributed::driver::DistributedDriver::run_job(
                            &worker_addrs,
                            &job,
                        )
                        .await;
                        let mut jobs = jobs.lock().await;
                        if let Some(entry) = jobs.get_mut(&job.job_id) {
                            match result {
                                Ok(vertices) => {
                                    entry.state = "finished".to_string();
                                    entry.result_vertices = Some(vertices);
                                }
                                Err(e) => {
                                    entry.state = "failed".to_string();
                                    entry.error = Some(format!("{e}"));
                                }
                            }
                        }
                    });
                }
                ClientToDriver::GetJobStatus { job_id } => {
                    let (state, err) = {
                        let jobs = self.jobs.lock().await;
                        match jobs.get(&job_id) {
                            None => ("not_found".to_string(), None),
                            Some(j) => (j.state.clone(), j.error.clone()),
                        }
                    };
                    if let Some(message) = err {
                        send_msg(&mut framed, &DriverToClient::Error { message }).await?;
                    } else {
                        send_msg(&mut framed, &DriverToClient::JobStatus { job_id, state }).await?;
                    }
                }
                ClientToDriver::FetchVertices { job_id } => {
                    let vertices = {
                        let jobs = self.jobs.lock().await;
                        jobs.get(&job_id).and_then(|j| j.result_vertices.clone())
                    };
                    match vertices {
                        Some(vertices) => {
                            send_msg(&mut framed, &DriverToClient::Vertices { job_id, vertices })
                                .await?;
                        }
                        None => {
                            send_msg(
                                &mut framed,
                                &DriverToClient::Error {
                                    message: "job not finished or not found".to_string(),
                                },
                            )
                            .await?;
                        }
                    }
                }
                ClientToDriver::Shutdown => {
                    break;
                }
            }
        }

        Ok(())
    }

    async fn resolve_workers(&self) -> GeaFlowResult<Vec<SocketAddr>> {
        if !self.config.worker_addrs.is_empty() {
            return Ok(self.config.worker_addrs.clone());
        }
        let master_addr = self
            .config
            .master_addr
            .ok_or_else(|| GeaFlowError::InvalidArgument("no workers and no master".to_string()))?;
        let stream = tokio::net::TcpStream::connect(master_addr)
            .await
            .map_err(|e| GeaFlowError::Internal(format!("connect master: {e}")))?;
        let mut framed = framed(stream);
        send_msg(
            &mut framed,
            &MasterRequest::Driver(DriverToMaster::GetWorkers),
        )
        .await?;
        let resp: MasterResponse = recv_msg(&mut framed).await?;
        match resp {
            MasterResponse::Workers { worker_addrs } => {
                let mut out = Vec::new();
                for s in worker_addrs {
                    out.push(s.parse().map_err(|e| {
                        GeaFlowError::InvalidArgument(format!("bad worker addr: {e}"))
                    })?);
                }
                Ok(out)
            }
            MasterResponse::Error { message } => Err(GeaFlowError::Internal(message)),
            _ => Err(GeaFlowError::Internal(
                "unexpected master response".to_string(),
            )),
        }
    }
}
