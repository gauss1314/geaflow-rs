use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;

pub async fn listen_lines(addr: SocketAddr, max_lines: usize) -> GeaFlowResult<Vec<String>> {
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| GeaFlowError::Internal(format!("bind socket source: {e}")))?;
    let (stream, _) = listener
        .accept()
        .await
        .map_err(|e| GeaFlowError::Internal(format!("accept socket source: {e}")))?;

    let mut reader = BufReader::new(stream);
    let mut lines = Vec::new();
    let mut buf = String::new();

    while lines.len() < max_lines {
        buf.clear();
        let n = reader
            .read_line(&mut buf)
            .await
            .map_err(|e| GeaFlowError::Internal(format!("read line: {e}")))?;
        if n == 0 {
            break;
        }
        lines.push(buf.trim_end_matches(&['\n', '\r'][..]).to_string());
    }
    Ok(lines)
}
