use clap::Parser;
use geaflow_runtime::distributed::driver_service::{DriverService, DriverServiceConfig};
use geaflow_runtime::http::{serve_http, HttpResponse};
use geaflow_runtime::observability::{init_prometheus, init_tracing};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    listen: SocketAddr,

    #[arg(long, value_delimiter = ',')]
    workers: Vec<SocketAddr>,

    #[arg(long)]
    master: Option<SocketAddr>,

    #[arg(long)]
    metrics_listen: Option<SocketAddr>,

    #[arg(long)]
    http_listen: Option<SocketAddr>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    init_tracing();
    if let Some(addr) = args.metrics_listen {
        let _handle = init_prometheus(addr)?;
    }

    let service = DriverService::new(DriverServiceConfig {
        listen_addr: args.listen,
        worker_addrs: args.workers,
        master_addr: args.master,
    });

    if let Some(addr) = args.http_listen {
        let svc = service.clone();
        tokio::spawn(async move {
            let handler: geaflow_runtime::http::HttpHandler = Arc::new(move |path: String| {
                let svc = svc.clone();
                Box::pin(async move {
                    match path.as_str() {
                        "/healthz" => HttpResponse {
                            status: 200,
                            content_type: "text/plain",
                            body: b"ok".to_vec(),
                        },
                        "/jobs" => {
                            let jobs = svc.list_jobs().await;
                            let body = serde_json::to_vec(&jobs).unwrap_or_else(|_| b"[]".to_vec());
                            HttpResponse {
                                status: 200,
                                content_type: "application/json",
                                body,
                            }
                        }
                        _ => HttpResponse {
                            status: 404,
                            content_type: "text/plain",
                            body: b"not found".to_vec(),
                        },
                    }
                })
                    as std::pin::Pin<Box<dyn std::future::Future<Output = HttpResponse> + Send>>
            });
            let _ = serve_http(addr, handler).await;
        });
    }

    service.run().await?;
    Ok(())
}
