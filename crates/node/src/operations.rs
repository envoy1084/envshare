//! Bounded loopback health, readiness, and `OpenMetrics` HTTP server.

use std::{net::SocketAddr, time::Duration};

use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
    sync::Semaphore,
};
use tokio_util::sync::CancellationToken;

use crate::{NodeError, NodeStatus};

const MAX_REQUEST_BYTES: usize = 8 * 1024;
const MAX_CONNECTIONS: usize = 32;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

/// Bound loopback operations endpoint.
pub struct OperationsServer {
    listener: TcpListener,
    status: NodeStatus,
}

impl OperationsServer {
    /// Binds a loopback-only operations endpoint.
    ///
    /// # Errors
    ///
    /// Returns if the address is not loopback or cannot be bound.
    pub async fn bind(address: SocketAddr, status: NodeStatus) -> Result<Self, NodeError> {
        if !address.ip().is_loopback() {
            return Err(NodeError::Configuration);
        }
        let listener = TcpListener::bind(address)
            .await
            .map_err(|_| NodeError::Operations)?;
        Ok(Self { listener, status })
    }

    /// Returns the concrete bound address, including an assigned ephemeral port.
    ///
    /// # Errors
    ///
    /// Returns if the local socket address is unavailable.
    pub fn local_addr(&self) -> Result<SocketAddr, NodeError> {
        self.listener
            .local_addr()
            .map_err(|_| NodeError::Operations)
    }

    /// Serves bounded requests until cancellation.
    ///
    /// # Errors
    ///
    /// Returns if accepting a connection fails.
    pub async fn run(self, cancellation: CancellationToken) -> Result<(), NodeError> {
        let capacity = std::sync::Arc::new(Semaphore::new(MAX_CONNECTIONS));
        let mut tasks = tokio::task::JoinSet::new();
        loop {
            let stream = tokio::select! {
                () = cancellation.cancelled() => {
                    tasks.abort_all();
                    while tasks.join_next().await.is_some() {}
                    return Ok(());
                },
                _ = tasks.join_next(), if !tasks.is_empty() => continue,
                accepted = self.listener.accept() => accepted
                    .map(|(stream, _)| stream)
                    .map_err(|_| NodeError::Operations)?,
            };
            let Ok(permit) = capacity.clone().try_acquire_owned() else {
                continue;
            };
            let status = self.status.clone();
            tasks.spawn(async move {
                let _permit = permit;
                let _ = serve_connection(stream, &status).await;
            });
        }
    }
}

async fn serve_connection(mut stream: TcpStream, status: &NodeStatus) -> Result<(), NodeError> {
    let mut request = [0_u8; MAX_REQUEST_BYTES];
    let read = tokio::time::timeout(IO_TIMEOUT, stream.read(&mut request))
        .await
        .map_err(|_| NodeError::Operations)?
        .map_err(|_| NodeError::Operations)?;
    let line = std::str::from_utf8(&request[..read])
        .ok()
        .and_then(|request| request.lines().next());
    let (code, content_type, body) = match line {
        Some("GET /healthz HTTP/1.1" | "GET /healthz HTTP/1.0") if status.is_live() => {
            ("200 OK", "text/plain; charset=utf-8", "ok\n".to_owned())
        }
        Some("GET /healthz HTTP/1.1" | "GET /healthz HTTP/1.0") => (
            "503 Service Unavailable",
            "text/plain; charset=utf-8",
            "unhealthy\n".to_owned(),
        ),
        Some("GET /readyz HTTP/1.1" | "GET /readyz HTTP/1.0") if status.is_ready() => {
            ("200 OK", "text/plain; charset=utf-8", "ready\n".to_owned())
        }
        Some("GET /readyz HTTP/1.1" | "GET /readyz HTTP/1.0") => (
            "503 Service Unavailable",
            "text/plain; charset=utf-8",
            "not ready\n".to_owned(),
        ),
        Some("GET /metrics HTTP/1.1" | "GET /metrics HTTP/1.0") => (
            "200 OK",
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
            status.metrics(),
        ),
        Some(line) if !line.starts_with("GET ") => (
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            "method not allowed\n".to_owned(),
        ),
        _ => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            "not found\n".to_owned(),
        ),
    };
    let response = format!(
        "HTTP/1.1 {code}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    tokio::time::timeout(IO_TIMEOUT, stream.write_all(response.as_bytes()))
        .await
        .map_err(|_| NodeError::Operations)?
        .map_err(|_| NodeError::Operations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_readiness_and_metrics_follow_node_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let status = NodeStatus::default();
        status.start();
        let server = OperationsServer::bind(([127, 0, 0, 1], 0).into(), status.clone()).await?;
        let address = server.local_addr()?;
        let cancellation = CancellationToken::new();
        let task = tokio::spawn(server.run(cancellation.clone()));

        assert!(
            request(address, "/healthz")
                .await?
                .starts_with("HTTP/1.1 200")
        );
        assert!(
            request(address, "/readyz")
                .await?
                .starts_with("HTTP/1.1 503")
        );
        status.listening();
        assert!(
            request(address, "/readyz")
                .await?
                .starts_with("HTTP/1.1 200")
        );
        let metrics = request(address, "/metrics").await?;
        assert!(metrics.contains("envshare_node_ready 1"));
        assert!(metrics.ends_with("# EOF\n"));
        status.begin_drain();
        assert!(
            request(address, "/readyz")
                .await?
                .starts_with("HTTP/1.1 503")
        );
        assert!(
            request(address, "/healthz")
                .await?
                .starts_with("HTTP/1.1 200")
        );

        cancellation.cancel();
        task.await??;
        Ok(())
    }

    async fn request(
        address: SocketAddr,
        path: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut stream = TcpStream::connect(address).await?;
        stream
            .write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
            .await?;
        let mut response = String::new();
        stream.read_to_string(&mut response).await?;
        Ok(response)
    }
}
