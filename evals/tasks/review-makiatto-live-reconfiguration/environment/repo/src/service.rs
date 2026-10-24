//! HTTP listener lifecycle and reload supervision.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Mutex, oneshot},
    task::JoinHandle,
};

use crate::{
    config::{Config, select_weighted_peer},
    wasm::Transforms,
};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("listener I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("server task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

struct RunningServer {
    stop: oneshot::Sender<()>,
    handle: JoinHandle<Result<(), std::io::Error>>,
    address: SocketAddr,
}

pub struct Service {
    running: Option<RunningServer>,
}

impl Service {
    pub async fn start(config: Config) -> Result<Self, ServiceError> {
        let transforms = Arc::new(Mutex::new(Transforms::default()));
        let running = spawn_server(config, Arc::clone(&transforms)).await?;
        Ok(Self {
            running: Some(running),
        })
    }

    pub fn address(&self) -> Option<SocketAddr> {
        self.running.as_ref().map(|server| server.address)
    }

    pub async fn shutdown(mut self) -> Result<(), ServiceError> {
        if let Some(server) = self.running.take() {
            let _ = server.stop.send(());
            server.handle.await??;
        }
        Ok(())
    }
}

async fn spawn_server(
    config: Config,
    transforms: Arc<Mutex<Transforms>>,
) -> Result<RunningServer, std::io::Error> {
    let listener = TcpListener::bind(config.listen).await?;
    let address = listener.local_addr()?;
    let (stop, mut stopping) = oneshot::channel();
    let handle = tokio::spawn(async move {
        let mut ticket = 0_u64;
        loop {
            tokio::select! {
                biased;
                _ = &mut stopping => break,
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    serve(stream, &config, &transforms, ticket).await?;
                    ticket = ticket.wrapping_add(1);
                }
            }
        }
        Ok(())
    });
    Ok(RunningServer {
        stop,
        handle,
        address,
    })
}

async fn serve(
    mut stream: TcpStream,
    config: &Config,
    transforms: &Mutex<Transforms>,
    ticket: u64,
) -> Result<(), std::io::Error> {
    let mut request = vec![0_u8; 4096];
    let received =
        match tokio::time::timeout(Duration::from_secs(30), stream.read(&mut request)).await {
            Ok(read) => read?,
            Err(_) => return Ok(()),
        };
    if received == 0 {
        return Ok(());
    }
    let output = if let Some(peer) = select_weighted_peer(&config.peers, ticket) {
        transforms
            .lock()
            .await
            .apply(&peer.transform, b"makiatto")
            .unwrap_or_else(|error| error.to_string().into_bytes())
    } else {
        b"makiatto".to_vec()
    };
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        output.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(&output).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            peers: vec![],
        }
    }

    async fn response(address: SocketAddr) -> String {
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(b"GET / HTTP/1.1\r\n\r\n").await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    #[tokio::test]
    async fn serves_and_joins_listener() {
        let service = Service::start(config()).await.unwrap();
        let address = service.address().unwrap();
        assert!(response(address).await.ends_with("makiatto"));
        service.shutdown().await.unwrap();
        assert!(TcpStream::connect(address).await.is_err());
    }
}
