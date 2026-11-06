//! A minimal SOCKS5 proxy for tests. Every connection goes to one fixed address, so a
//! test can use a host name that doesn't exist and prove the proxy was really used.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};

/// A running proxy. Dropping it stops the proxy.
pub struct SocksProxy {
    addr: SocketAddr,
    connections: Arc<AtomicU32>,
    accept_loop: JoinHandle<()>,
}

impl SocksProxy {
    pub async fn start(forward_to: SocketAddr) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let addr = listener.local_addr()?;
        let connections = Arc::new(AtomicU32::new(0));

        let counter = Arc::clone(&connections);
        let accept_loop = tokio::spawn(async move {
            let mut tunnels = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        if let Ok((stream, _)) = accepted {
                            counter.fetch_add(1, Ordering::Relaxed);
                            tunnels.spawn(tunnel(stream, forward_to));
                        }
                    }
                    Some(_) = tunnels.join_next() => {}
                }
            }
        });

        Ok(Self {
            addr,
            connections,
            accept_loop,
        })
    }

    pub fn url(&self) -> String {
        format!("socks5h://{}", self.addr)
    }

    pub fn connections(&self) -> u32 {
        self.connections.load(Ordering::Relaxed)
    }
}

impl Drop for SocksProxy {
    fn drop(&mut self) {
        self.accept_loop.abort();
    }
}

async fn tunnel(mut client: TcpStream, forward_to: SocketAddr) -> io::Result<()> {
    // Greeting: version, method count, methods. Answer: version 5, no login.
    let mut header = [0u8; 2];
    client.read_exact(&mut header).await?;
    let mut methods = vec![0u8; usize::from(header[1])];
    client.read_exact(&mut methods).await?;
    client.write_all(&[5, 0]).await?;

    // Request: version, command, reserved, address type, then the address and port.
    let mut request = [0u8; 4];
    client.read_exact(&mut request).await?;
    let address_length = match request[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut length = [0u8; 1];
            client.read_exact(&mut length).await?;
            usize::from(length[0])
        }
        _ => return Ok(()),
    };
    let mut rest = vec![0u8; address_length + 2];
    client.read_exact(&mut rest).await?;

    let mut server = TcpStream::connect(forward_to).await?;
    client.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;

    copy_bidirectional(&mut client, &mut server).await?;
    Ok(())
}
