use clap::Parser;
use geaflow_runtime::distributed::master::{MasterConfig, MasterService};
use geaflow_runtime::http::{serve_http, HttpResponse};
use geaflow_runtime::observability::{init_prometheus, init_tracing};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    listen: SocketAddr,

    #[arg(long, default_value_t = 5000)]
    worker_ttl_ms: u64,

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

    let master = MasterService::new(MasterConfig {
        listen_addr: args.listen,
        worker_ttl_ms: args.worker_ttl_ms,
    });

    if let Some(addr) = args.http_listen {
        let master_http = master.clone();
        tokio::spawn(async move {
            let handler: geaflow_runtime::http::HttpHandler = Arc::new(move |path: String| {
                let master_http = master_http.clone();
                Box::pin(async move {
                    match path.as_str() {
                        "/healthz" => HttpResponse {
                            status: 200,
                            content_type: "text/plain",
                            body: b"ok".to_vec(),
                        },
                        "/workers" => {
                            let workers = master_http.list_workers().await;
                            let body =
                                serde_json::to_vec(&workers).unwrap_or_else(|_| b"[]".to_vec());
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
    master.run().await?;
    Ok(())
}
