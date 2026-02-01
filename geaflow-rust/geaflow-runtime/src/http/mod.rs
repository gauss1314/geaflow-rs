use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub struct HttpResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

pub type HttpHandler = Arc<
    dyn Fn(String) -> Pin<Box<dyn std::future::Future<Output = HttpResponse> + Send>> + Send + Sync,
>;

pub async fn serve_http(addr: SocketAddr, handler: HttpHandler) -> GeaFlowResult<()> {
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| GeaFlowError::Internal(format!("bind http: {e}")))?;

    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|e| GeaFlowError::Internal(format!("accept http: {e}")))?;

        let handler_ref = handler.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            let n = match stream.read(&mut buf).await {
                Ok(n) => n,
                Err(_) => return,
            };
            if n == 0 {
                return;
            }
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = parse_path(&req).unwrap_or_else(|| "/".to_string());
            let resp = (handler_ref)(path).await;
            let _ = write_response(&mut stream, resp).await;
        });
    }
}

fn parse_path(req: &str) -> Option<String> {
    let mut lines = req.lines();
    let first = lines.next()?;
    let mut parts = first.split_whitespace();
    let method = parts.next()?;
    if method != "GET" {
        return None;
    }
    let path = parts.next()?;
    Some(path.to_string())
}

async fn write_response(
    stream: &mut tokio::net::TcpStream,
    resp: HttpResponse,
) -> GeaFlowResult<()> {
    let status_line = match resp.status {
        200 => "HTTP/1.1 200 OK",
        404 => "HTTP/1.1 404 Not Found",
        500 => "HTTP/1.1 500 Internal Server Error",
        _ => "HTTP/1.1 200 OK",
    };

    let header = format!(
        "{status_line}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        resp.content_type,
        resp.body.len()
    );
    stream
        .write_all(header.as_bytes())
        .await
        .map_err(|e| GeaFlowError::Internal(format!("write http header: {e}")))?;
    stream
        .write_all(&resp.body)
        .await
        .map_err(|e| GeaFlowError::Internal(format!("write http body: {e}")))?;
    Ok(())
}
