//! ZKS-Tunnel Client - Local SOCKS5 Proxy
//!
//! This CLI tool creates a local SOCKS5 proxy server that tunnels
//! all traffic through the ZKS-Tunnel Worker.
//!
//! Usage:
//!   zks-vpn --worker wss://zks-tunnel.user.workers.dev/tunnel --port 1080
//!
//! Then configure your browser/system to use SOCKS5 proxy at localhost:1080

use clap::Parser;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{info, error, Level};
use tracing_subscriber::FmtSubscriber;

mod socks5;
mod tunnel;
mod stream_manager;

use tunnel::TunnelClient;
use socks5::Socks5Server;

/// ZKS-Tunnel VPN Client
#[derive(Parser, Debug)]
#[command(name = "zks-vpn")]
#[command(author = "Md Wasif Faisal")]
#[command(version = "0.1.0")]
#[command(about = "Serverless VPN via Cloudflare Workers", long_about = None)]
struct Args {
    /// ZKS-Tunnel Worker WebSocket URL
    #[arg(short, long, default_value = "wss://zks-tunnel.workers.dev/tunnel")]
    worker: String,

    /// Local SOCKS5 proxy port
    #[arg(short, long, default_value_t = 1080)]
    port: u16,

    /// Bind address
    #[arg(short, long, default_value = "127.0.0.1")]
    bind: String,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let args = Args::parse();

    // Initialize logging
    let level = if args.verbose { Level::DEBUG } else { Level::INFO };
    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║         ZKS-Tunnel VPN - Serverless & Free                   ║");
    info!("╠══════════════════════════════════════════════════════════════╣");
    info!("║  Worker: {}  ", args.worker);
    info!("║  SOCKS5: {}:{}                                ", args.bind, args.port);
    info!("╚══════════════════════════════════════════════════════════════╝");

    // Connect to Worker
    info!("Connecting to ZKS-Tunnel Worker...");
    let tunnel = TunnelClient::connect_ws(&args.worker).await.map_err(|e| {
        error!("❌ Failed to connect: {}", e);
        e
    })?;
    info!("✅ Connected to Worker!");

    // Start SOCKS5 server
    let bind_addr: SocketAddr = format!("{}:{}", args.bind, args.port).parse()?;
    let listener = TcpListener::bind(bind_addr).await?;
    
    info!("🚀 SOCKS5 proxy listening on {}", bind_addr);
    info!("   Configure your browser to use SOCKS5 proxy: {}:{}", args.bind, args.port);

    let socks_server = Socks5Server::new(tunnel);
    socks_server.run(listener).await?;

    Ok(())
}

