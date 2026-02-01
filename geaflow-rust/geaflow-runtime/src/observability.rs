use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

pub fn init_prometheus(addr: SocketAddr) -> GeaFlowResult<PrometheusHandle> {
    PrometheusBuilder::new()
        .with_http_listener(addr)
        .install_recorder()
        .map_err(|e| GeaFlowError::Internal(format!("prometheus init: {e}")))
}
