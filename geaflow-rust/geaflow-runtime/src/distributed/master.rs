use crate::distributed::protocol::{
    framed, recv_msg, send_msg, MasterRequest, MasterResponse, WorkerToMaster,
};
use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct MasterConfig {
    pub listen_addr: SocketAddr,
    pub worker_ttl_ms: u64,
}

#[derive(Clone)]
pub struct MasterService {
    config: MasterConfig,
    workers: Arc<Mutex<HashMap<String, u128>>>,
}

impl MasterService {
    pub fn new(config: MasterConfig) -> Self {
        Self {
            config,
            workers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn run(&self) -> GeaFlowResult<()> {
        let listener = TcpListener::bind(self.config.listen_addr)
            .await
            .map_err(|e| GeaFlowError::Internal(format!("bind master: {e}")))?;

        loop {
            let (stream, _) = listener
                .accept()
                .await
                .map_err(|e| GeaFlowError::Internal(format!("accept: {e}")))?;
            let svc = self.clone();
            tokio::spawn(async move {
                let _ = svc.handle_connection(stream).await;
            });
        }
    }

    pub async fn list_workers(&self) -> Vec<String> {
        self.alive_workers().await
    }

    async fn handle_connection(&self, stream: tokio::net::TcpStream) -> GeaFlowResult<()> {
        let mut framed = framed(stream);
        loop {
            let req: MasterRequest = recv_msg(&mut framed).await?;
            match req {
                MasterRequest::Worker(msg) => {
                    self.handle_worker(msg).await?;
                    send_msg(&mut framed, &MasterResponse::Ack).await?;
                }
                MasterRequest::Driver(msg) => match msg {
                    crate::distributed::protocol::DriverToMaster::GetWorkers => {
                        let workers = self.alive_workers().await;
                        send_msg(
                            &mut framed,
                            &MasterResponse::Workers {
                                worker_addrs: workers,
                            },
                        )
                        .await?;
                    }
                },
            }
        }
    }

    async fn handle_worker(&self, msg: WorkerToMaster) -> GeaFlowResult<()> {
        let now = now_nanos();
        match msg {
            WorkerToMaster::Register { worker_addr } => {
                let mut w = self.workers.lock().await;
                w.insert(worker_addr, now);
            }
            WorkerToMaster::Heartbeat { worker_addr } => {
                let mut w = self.workers.lock().await;
                w.insert(worker_addr, now);
            }
        }
        Ok(())
    }

    async fn alive_workers(&self) -> Vec<String> {
        let now = now_nanos();
        let ttl_ns = (self.config.worker_ttl_ms as u128) * 1_000_000;
        let mut w = self.workers.lock().await;
        w.retain(|_, last| now.saturating_sub(*last) <= ttl_ns);
        let mut out: Vec<String> = w.keys().cloned().collect();
        out.sort();
        out
    }
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
