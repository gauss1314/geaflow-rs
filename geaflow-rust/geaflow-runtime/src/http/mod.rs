use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use std::collections::HashMap;
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

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub type HttpHandlerV2 = Arc<
    dyn Fn(HttpRequest) -> Pin<Box<dyn std::future::Future<Output = HttpResponse> + Send>>
        + Send
        + Sync,
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

pub async fn serve_http_v2(addr: SocketAddr, handler: HttpHandlerV2) -> GeaFlowResult<()> {
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
            let req = match read_request(&mut stream).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = write_response(
                        &mut stream,
                        HttpResponse {
                            status: 500,
                            content_type: "text/plain",
                            body: format!("{e}").into_bytes(),
                        },
                    )
                    .await;
                    return;
                }
            };
            let resp = (handler_ref)(req).await;
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

async fn read_request(stream: &mut tokio::net::TcpStream) -> GeaFlowResult<HttpRequest> {
    const MAX_HEADER_BYTES: usize = 64 * 1024;
    const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

    let mut buf = Vec::<u8>::with_capacity(8192);
    let mut tmp = [0u8; 8192];
    let header_end;
    loop {
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|e| GeaFlowError::Internal(format!("read http: {e}")))?;
        if n == 0 {
            return Err(GeaFlowError::Internal("http eof".to_string()));
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() > MAX_HEADER_BYTES {
            return Err(GeaFlowError::InvalidArgument(
                "http header too large".to_string(),
            ));
        }
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
    }

    let header_str = String::from_utf8_lossy(&buf[..header_end]);
    let mut lines = header_str.split("\r\n");
    let first = lines
        .next()
        .ok_or_else(|| GeaFlowError::InvalidArgument("bad http request".to_string()))?;
    let mut parts = first.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| GeaFlowError::InvalidArgument("bad http request".to_string()))?
        .to_string();
    let path = parts
        .next()
        .ok_or_else(|| GeaFlowError::InvalidArgument("bad http request".to_string()))?
        .to_string();

    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    let content_length = headers
        .get("content-length")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err(GeaFlowError::InvalidArgument(
            "http body too large".to_string(),
        ));
    }

    let mut body = Vec::with_capacity(content_length);
    if buf.len() > header_end {
        let available = &buf[header_end..];
        let take = available.len().min(content_length);
        body.extend_from_slice(&available[..take]);
    }

    while body.len() < content_length {
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|e| GeaFlowError::Internal(format!("read http body: {e}")))?;
        if n == 0 {
            return Err(GeaFlowError::Internal("http body eof".to_string()));
        }
        let need = content_length - body.len();
        body.extend_from_slice(&tmp[..n.min(need)]);
    }

    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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
