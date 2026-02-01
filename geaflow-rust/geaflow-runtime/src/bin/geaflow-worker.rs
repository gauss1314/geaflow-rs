use clap::Parser;
use geaflow_runtime::distributed::worker::{run_worker, WorkerConfig};
use geaflow_runtime::observability::{init_prometheus, init_tracing};
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    listen: SocketAddr,

    #[arg(long)]
    state_dir: PathBuf,

    #[arg(long)]
    metrics_listen: Option<SocketAddr>,

    #[arg(long)]
    master: Option<SocketAddr>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    init_tracing();
    if let Some(addr) = args.metrics_listen {
        let _handle = init_prometheus(addr)?;
    }
    run_worker(WorkerConfig {
        listen_addr: args.listen,
        state_dir: args.state_dir,
        master_addr: args.master,
    })
    .await?;
    Ok(())
}
