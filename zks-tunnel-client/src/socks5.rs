//! SOCKS5 Protocol Implementation
//!
//! Implements RFC 1928 (SOCKS5) for proxying TCP connections.
//! Only supports CONNECT command (not BIND or UDP ASSOCIATE).

use crate::tunnel::TunnelClient;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info};

/// SOCKS5 versions
const SOCKS_VERSION: u8 = 0x05;

/// SOCKS5 authentication methods
const AUTH_NO_AUTH: u8 = 0x00;

/// SOCKS5 commands
const CMD_CONNECT: u8 = 0x01;

/// SOCKS5 address types
const ATYP_IPV4: u8 = 0x01;
const ATYP_DOMAIN: u8 = 0x03;
const ATYP_IPV6: u8 = 0x04;

/// SOCKS5 reply codes
const REP_SUCCESS: u8 = 0x00;
#[allow(dead_code)]
const REP_GENERAL_FAILURE: u8 = 0x01;
const REP_HOST_UNREACHABLE: u8 = 0x04;
const REP_CMD_NOT_SUPPORTED: u8 = 0x07;
const REP_ATYP_NOT_SUPPORTED: u8 = 0x08;

pub struct Socks5Server {
    tunnel: Arc<TunnelClient>,
}

impl Socks5Server {
    pub fn new(tunnel: TunnelClient) -> Self {
        Self {
            tunnel: Arc::new(tunnel),
        }
    }

    pub async fn run(
        &self,
        listener: TcpListener,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        loop {
            let (stream, addr) = listener.accept().await?;
            debug!("New SOCKS5 connection from {}", addr);

            let tunnel = self.tunnel.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_socks5_connection(stream, tunnel).await {
                    error!("SOCKS5 error: {}", e);
                }
            });
        }
    }
}

async fn handle_socks5_connection(
    mut stream: TcpStream,
    tunnel: Arc<TunnelClient>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Step 1: Version identification / method selection
    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf).await?;

    if buf[0] != SOCKS_VERSION {
        return Err(format!("Invalid SOCKS version: {}", buf[0]).into());
    }

    let nmethods = buf[1] as usize;
    let mut methods = vec![0u8; nmethods];
    stream.read_exact(&mut methods).await?;

    // We only support no-auth
    if !methods.contains(&AUTH_NO_AUTH) {
        stream.write_all(&[SOCKS_VERSION, 0xFF]).await?;
        return Err("No supported auth method".into());
    }

    // Accept no-auth
    stream.write_all(&[SOCKS_VERSION, AUTH_NO_AUTH]).await?;

    // Step 2: Request
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;

    if header[0] != SOCKS_VERSION {
        return Err("Invalid SOCKS version in request".into());
    }

    let cmd = header[1];
    let atyp = header[3];

    if cmd != CMD_CONNECT {
        // Only CONNECT is supported
        send_reply(&mut stream, REP_CMD_NOT_SUPPORTED, "0.0.0.0", 0).await?;
        return Err("Only CONNECT command supported".into());
    }

    // Parse destination address
    let (host, port) = match atyp {
        ATYP_IPV4 => {
            let mut addr = [0u8; 4];
            stream.read_exact(&mut addr).await?;
            let mut port_buf = [0u8; 2];
            stream.read_exact(&mut port_buf).await?;
            let port = u16::from_be_bytes(port_buf);
            let host = format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3]);
            (host, port)
        }
        ATYP_DOMAIN => {
            let mut len_buf = [0u8; 1];
            stream.read_exact(&mut len_buf).await?;
            let len = len_buf[0] as usize;
            let mut domain = vec![0u8; len];
            stream.read_exact(&mut domain).await?;
            let mut port_buf = [0u8; 2];
            stream.read_exact(&mut port_buf).await?;
            let port = u16::from_be_bytes(port_buf);
            let host = String::from_utf8(domain)?;
            (host, port)
        }
        ATYP_IPV6 => {
            send_reply(&mut stream, REP_ATYP_NOT_SUPPORTED, "0.0.0.0", 0).await?;
            return Err("IPv6 not yet supported".into());
        }
        _ => {
            send_reply(&mut stream, REP_ATYP_NOT_SUPPORTED, "0.0.0.0", 0).await?;
            return Err(format!("Unknown address type: {}", atyp).into());
        }
    };

    info!("SOCKS5 CONNECT to {}:{}", host, port);

    // Step 3: Connect via tunnel
    match tunnel.open_stream(&host, port).await {
        Ok((stream_id, rx)) => {
            info!("✅ Tunnel connected: stream_id={}", stream_id);
            send_reply(&mut stream, REP_SUCCESS, "0.0.0.0", 0).await?;

            // Step 4: Relay data between local socket and tunnel (bidirectional)
            tunnel.relay(stream_id, stream, rx).await?;
        }
        Err(e) => {
            error!("❌ Tunnel connect failed: {}", e);
            send_reply(&mut stream, REP_HOST_UNREACHABLE, "0.0.0.0", 0).await?;
        }
    }

    Ok(())
}

async fn send_reply(
    stream: &mut TcpStream,
    rep: u8,
    bind_addr: &str,
    bind_port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr_parts: Vec<u8> = bind_addr
        .split('.')
        .filter_map(|s| s.parse::<u8>().ok())
        .collect();

    let reply = [
        SOCKS_VERSION,
        rep,
        0x00, // Reserved
        ATYP_IPV4,
        addr_parts.first().copied().unwrap_or(0),
        addr_parts.get(1).copied().unwrap_or(0),
        addr_parts.get(2).copied().unwrap_or(0),
        addr_parts.get(3).copied().unwrap_or(0),
        (bind_port >> 8) as u8,
        (bind_port & 0xFF) as u8,
    ];

    stream.write_all(&reply).await?;
    Ok(())
}
