//! ZKS-Tunnel Client - Local SOCKS5 Proxy & System-Wide VPN
//!
//! This CLI tool provides two modes:
//! 1. SOCKS5 Proxy (default): Creates a local proxy for browser traffic
//! 2. VPN Mode: Routes ALL system traffic through the tunnel
//!
//! Usage:
//!   # SOCKS5 mode (default)
//!   zks-vpn --worker wss://zks-tunnel.user.workers.dev/tunnel
//!
//!   # System-wide VPN mode (requires admin/root)
//!   zks-vpn --worker wss://zks-tunnel.user.workers.dev/tunnel --mode vpn
//!
//! Then configure your browser/system to use SOCKS5 proxy at localhost:1080

use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

use clap::Parser;
use zks_tunnel_client::cli::{Args, Mode};
use zks_tunnel_client::utils::{BoxError, check_privileges};
use zks_tunnel_client::p2p_vpn::start_p2p_vpn;

use zks_tunnel_client::{
    entry_node, exit_node_udp, exit_peer, file_transfer, http_proxy,
    hybrid_data, p2p_client, p2p_relay, socks5,
    tunnel,
};

#[cfg(feature = "vpn")]
use zks_tunnel_client::vpn;


#[cfg(windows)]
use zks_tunnel_client::windows_service;

#[cfg(feature = "swarm")]
use zks_tunnel_client::{p2p_swarm, swarm, onion, signaling, swarm_controller};

use http_proxy::HttpProxyServer;
use socks5::Socks5Server;
use tunnel::TunnelClient;

#[cfg(feature = "vpn")]
use std::sync::Arc;
#[cfg(feature = "vpn")]
use vpn::{VpnConfig, VpnController};

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let args = Args::parse();

    #[cfg(windows)]
    {
        if args.install_service {
            return windows_service::service::install_service();
        }
        if args.uninstall_service {
            return windows_service::service::uninstall_service();
        }
        if args.service {
            return windows_service::service::run().map_err(|e| e.into());
        }
    }

    // Initialize logging
    let level = if args.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };
    let subscriber = FmtSubscriber::builder().with_max_level(level).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Display banner
    print_banner(&args);

    // Handle P2P modes separately (they use relay, not tunnel worker)
    match args.mode {
        Mode::P2pClient => {
            let room_id = args.room.clone().unwrap_or_else(|| {
                error!("Room ID required for P2P mode. Use --room <id>");
                std::process::exit(1);
            });
            return p2p_client::run_p2p_client(
                &args.relay,
                &args.vernam,
                &room_id,
                args.port,
                args.proxy,
            )
            .await;
        }
        Mode::P2pVpn => {
            let room_id = args.room.clone().unwrap_or_else(|| {
                error!("Room ID required for P2P VPN mode. Use --room <id>");
                std::process::exit(1);
            });
            return run_p2p_vpn_mode(args, room_id).await;
        }
        Mode::ExitPeer => {
            let room_id = args.room.clone().unwrap_or_else(|| {
                error!("Room ID required for Exit Peer mode. Use --room <id>");
                std::process::exit(1);
            });
            info!("Running Exit Peer in SOCKS5/TCP mode (no TUN device)");
            return exit_peer::run_exit_peer(&args.relay, &args.vernam, &room_id).await;
        }
        Mode::ExitPeerVpn => {
            let room_id = args.room.clone().unwrap_or_else(|| {
                error!("Room ID required for Exit Peer VPN mode. Use --room <id>");
                std::process::exit(1);
            });

            #[cfg(feature = "vpn")]
            {
                info!("Running Exit Peer in VPN mode (TUN device enabled)");
                return exit_peer::run_exit_peer_vpn(&args.relay, &args.vernam, &room_id).await;
            }
            #[cfg(not(feature = "vpn"))]
            {
                error!("❌ Exit Peer VPN mode requires 'vpn' feature!");
                error!("   Rebuild with: cargo build --release --features vpn");
                return Err("VPN feature not enabled".into());
            }
        }
        Mode::EntryNode => {
            use entry_node::EntryNodeConfig;
            let listen_addr: std::net::SocketAddr =
                format!("0.0.0.0:{}", args.listen_port).parse()?;
            let exit_node_addr: std::net::SocketAddr = args.exit_node.parse().map_err(|_| {
                error!("Invalid exit node address: {}", args.exit_node);
                "Invalid exit node address"
            })?;
            return entry_node::run_entry_node(EntryNodeConfig {
                listen_addr,
                exit_node_addr,
            })
            .await;
        }
        Mode::ExitNodeUdp => {
            return exit_node_udp::run_exit_node_udp(args.listen_port).await;
        }
        Mode::ExitPeerHybrid => {
            let room_id = args.room.clone().unwrap_or_else(|| "default".to_string());
            #[cfg(feature = "vpn")]
            {
                return run_exit_peer_hybrid_mode(args, room_id).await;
            }
            #[cfg(not(feature = "vpn"))]
            {
                error!("❌ Hybrid Exit Peer mode requires 'vpn' feature!");
                error!("   Rebuild with: cargo build --release --features vpn");
                return Err("VPN feature not enabled".into());
            }
        }
        #[cfg(feature = "swarm")]
        Mode::Swarm => {
            let room_id = args.room.clone().unwrap_or_else(|| "default".to_string());
            return run_swarm_mode(args, room_id).await;
        }
        Mode::SendFile => {
            let room_id = args.room.clone().unwrap_or_else(|| {
                error!("Room ID required for file transfer. Use --room <id>");
                std::process::exit(1);
            });
            let file_path = args.file.clone().unwrap_or_else(|| {
                error!("File path required. Use --file <path>");
                std::process::exit(1);
            });
            return file_transfer::run_send_file(
                &args.relay,
                &args.vernam,
                &room_id,
                &file_path,
                args.dest,
            )
            .await;
        }
        Mode::ReceiveFile => {
            let room_id = args.room.clone().unwrap_or_else(|| "default".to_string());
            return file_transfer::run_receive_file(
                &args.relay,
                &args.vernam,
                &room_id,
                args.ticket,
            )
            .await;
        }
        _ => {}
    }

    // For other modes, connect to Worker
    info!("Connecting to ZKS-Tunnel Worker...");
    let tunnel = TunnelClient::connect_ws(&args.worker).await.map_err(|e| {
        error!("❌ Failed to connect: {}", e);
        e
    })?;
    info!("✅ Connected to Worker!");

    match args.mode {
        Mode::Socks5 => run_socks5_mode(args, tunnel).await,
        Mode::Http => run_http_proxy_mode(args, tunnel).await,
        Mode::Vpn => run_vpn_mode(args, tunnel).await,
        Mode::P2pClient
        | Mode::P2pVpn
        | Mode::ExitPeer
        | Mode::ExitPeerVpn
        | Mode::EntryNode
        | Mode::ExitNodeUdp
        | Mode::ExitPeerHybrid
        | Mode::SendFile
        | Mode::ReceiveFile => {
            unreachable!()
        }
        #[cfg(feature = "swarm")]
        Mode::Swarm => {
            unreachable!()
        }
    }
}

/// Print the application banner
fn print_banner(args: &Args) {
    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║         ZKS-Tunnel VPN - Serverless & Free                   ║");
    info!("╠══════════════════════════════════════════════════════════════╣");
    info!("║  Worker: {}  ", args.worker);

    match args.mode {
        Mode::Socks5 => {
            info!("║  Mode:   SOCKS5 Proxy (browser only)                        ║");
            info!(
                "║  Listen: {}:{}                                  ",
                args.bind, args.port
            );
        }
        Mode::Http => {
            info!("║  Mode:   HTTP Proxy (HTTPS via fetch, all sites work)      ║");
            info!(
                "║  Listen: {}:{}                                  ",
                args.bind, args.port
            );
        }
        Mode::Vpn => {
            info!("║  Mode:   System-Wide VPN (all traffic)                      ║");
            info!(
                "║  TUN:    {}                                             ",
                args.tun_name
            );
            info!(
                "║  VPN IP: {}                                          ",
                args.vpn_address.as_deref().unwrap_or("auto")
            );
        }
        Mode::P2pClient => {
            info!("║  Mode:   P2P Client (via Exit Peer)                         ║");
            info!("║  Room:   {}  ", args.room.as_deref().unwrap_or("none"));
            info!(
                "║  Listen: {}:{}                                  ",
                args.bind, args.port
            );
        }
        Mode::ExitPeer => {
            info!("║  Mode:   Exit Peer (forward to Internet)                    ║");
            info!("║  Room:   {}  ", args.room.as_deref().unwrap_or("none"));
        }
        Mode::ExitPeerVpn => {
            info!("║  Mode:   Exit Peer VPN (Layer 3 Forwarding)                 ║");
            info!("║  Room:   {}  ", args.room.as_deref().unwrap_or("none"));
        }
        Mode::P2pVpn => {
            info!("║  Mode:   P2P VPN (Triple-Blind, System-Wide)                ║");
            info!("║  Room:   {}  ", args.room.as_deref().unwrap_or("none"));
            info!(
                "║  VPN IP: {}                                          ",
                args.vpn_address.as_deref().unwrap_or("auto")
            );
        }
        Mode::EntryNode => {
            info!("║  Mode:   Entry Node (UDP Relay, Multi-Hop VPN)              ║");
            info!(
                "║  Listen: 0.0.0.0:{}                                      ",
                args.listen_port
            );
            info!("║  Exit:   {}  ", args.exit_node);
        }
        Mode::ExitNodeUdp => {
            info!("║  Mode:   Exit Node UDP (TUN, Multi-Hop VPN)                 ║");
            info!(
                "║  Listen: 0.0.0.0:{}                                      ",
                args.listen_port
            );
        }
        Mode::ExitPeerHybrid => {
            info!("║  Mode:   Exit Peer Hybrid (Worker + Tunnel)                ║");
            info!("║  Room:   {}  ", args.room.as_deref().unwrap_or("none"));
            info!("║  Data:   TCP port 51821 (via Cloudflare Tunnel)            ║");
        }
        Mode::SendFile => {
            info!("║  Mode:   Send File (P2P Encrypted)                          ║");
            info!("║  Room:   {}  ", args.room.as_deref().unwrap_or("none"));
            info!("║  File:   {}  ", args.file.as_deref().unwrap_or("none"));
        }
        Mode::ReceiveFile => {
            info!("║  Mode:   Receive File (P2P Encrypted)                       ║");
            info!("║  Room:   {}  ", args.room.as_deref().unwrap_or("none"));
        }
        #[cfg(feature = "swarm")]
        Mode::Swarm => {
            info!("║  Mode:   Faisal Swarm (P2P Mesh + DCUtR)                    ║");
            info!("║  Room:   {}  ", args.room.as_deref().unwrap_or("none"));
            info!("║  Roles:  Client + Relay + Exit (bandwidth sharing)         ║");
        }
    }

    info!("╚══════════════════════════════════════════════════════════════╝");
}

/// Run in SOCKS5 proxy mode
async fn run_socks5_mode(args: Args, tunnel: TunnelClient) -> Result<(), BoxError> {
    let bind_addr: SocketAddr = format!("{}:{}", args.bind, args.port).parse()?;
    let listener = TcpListener::bind(bind_addr).await?;

    info!("🚀 SOCKS5 proxy listening on {}", bind_addr);
    info!(
        "   Configure your browser to use SOCKS5 proxy: {}:{}",
        args.bind, args.port
    );
    info!("");
    info!("   Firefox: Settings → Network → Manual proxy → SOCKS5");
    info!("   Chrome:  Use SwitchyOmega extension");

    let socks_server = Socks5Server::new(tunnel);
    socks_server.run(listener).await?;

    Ok(())
}

/// Run in HTTP proxy mode (uses fetch() for HTTPS)
async fn run_http_proxy_mode(args: Args, tunnel: TunnelClient) -> Result<(), BoxError> {
    let bind_addr: SocketAddr = format!("{}:{}", args.bind, args.port).parse()?;
    let listener = TcpListener::bind(bind_addr).await?;

    info!("🚀 HTTP proxy listening on {}", bind_addr);
    info!(
        "   Configure your browser to use HTTP proxy: {}:{}",
        args.bind, args.port
    );
    info!("");
    info!("   ✅ HTTPS sites work via Cloudflare fetch() API");
    info!("   ✅ All Cloudflare-proxied sites are accessible");

    let http_server = HttpProxyServer::new(tunnel);
    http_server.run(listener).await?;

    Ok(())
}

/// Run in system-wide VPN mode
async fn run_vpn_mode(_args: Args, _tunnel: TunnelClient) -> Result<(), BoxError> {
    // Check if VPN feature is enabled
    #[cfg(not(feature = "vpn"))]
    {
        error!("❌ VPN mode is not enabled!");
        error!("   Rebuild with: cargo build --release --features vpn");
        Err("VPN feature not enabled".into())
    }

    #[cfg(feature = "vpn")]
    {
        // Check for admin/root privileges
        check_privileges()?;

        let vpn_addr: std::net::Ipv4Addr = _args
            .vpn_address
            .clone()
            .unwrap_or("10.0.85.1".to_string())
            .parse()?;

        let config = VpnConfig {
            device_name: _args.tun_name.clone(),
            address: vpn_addr,
            netmask: std::net::Ipv4Addr::new(255, 255, 255, 0),
            mtu: 1500,
            dns_protection: _args.dns_protection,
            kill_switch: _args.kill_switch,
        };

        info!("🔒 Starting system-wide VPN...");
        info!("   All traffic will be routed through the tunnel.");

        if _args.kill_switch {
            info!("   Kill switch: ENABLED (traffic blocked if VPN drops)");
        }

        if _args.dns_protection {
            info!("   DNS protection: ENABLED (queries via DoH)");
        }

        let tunnel = Arc::new(_tunnel);
        let vpn = VpnController::new(tunnel, config);

        vpn.start().await?;

        // Wait for Ctrl+C
        info!("");
        info!("Press Ctrl+C to disconnect VPN...");

        tokio::signal::ctrl_c().await?;

        info!("");
        info!("Shutting down VPN...");
        vpn.stop().await?;

        Ok(())
    }
}

/// Run in P2P VPN mode (Triple-Blind Architecture)
async fn run_p2p_vpn_mode(args: Args, room_id: String) -> Result<(), BoxError> {
    #[cfg(not(feature = "vpn"))]
    {
        error!("❌ VPN mode is not enabled!");
        error!("   Rebuild with: cargo build --release --features vpn");
        Err("VPN feature not enabled".into())
    }

    #[cfg(feature = "vpn")]
    {
        let vpn = start_p2p_vpn(args, room_id).await?;

        // Wait for Ctrl+C
        info!("");
        info!("Press Ctrl+C to disconnect VPN...");

        tokio::signal::ctrl_c().await?;

        info!("");
        info!("Shutting down P2P VPN...");
        vpn.stop().await?;

        Ok(())
    }
}



/// Run as Exit Peer in Hybrid mode (Worker signaling + Cloudflare Tunnel data)
///
/// This mode uses:
/// - Cloudflare Worker for signaling (key exchange, room management)  
/// - Cloudflare Tunnel for data (TCP port 51821, unlimited bandwidth)
#[cfg(feature = "vpn")]
async fn run_exit_peer_hybrid_mode(_args: Args, room_id: String) -> Result<(), BoxError> {
    use hybrid_data::{run_hybrid_data_listener, HybridDataState};
    use p2p_relay::{P2PRelay, PeerRole};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // Check privileges for TUN device
    check_privileges()?;

    info!("🚀 Starting Hybrid Exit Peer Mode...");
    info!("   Signaling: WebSocket via Cloudflare Worker");
    info!("   Data: TCP port 51821 via Cloudflare Tunnel");

    // Create TUN device
    let device = tun_rs::DeviceBuilder::new()
        .ipv4(std::net::Ipv4Addr::new(10, 0, 85, 2), 24, None)
        .mtu(1400)
        .build_async()?;

    info!("✅ TUN device created (10.0.85.2/24)");

    // Enable IP forwarding and NAT on Linux
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("sysctl")
            .args(["-w", "net.ipv4.ip_forward=1"])
            .output();
        info!("Enabled IP forwarding");

        let _ = std::process::Command::new("iptables")
            .args([
                "-t",
                "nat",
                "-A",
                "POSTROUTING",
                "-s",
                "10.0.85.0/24",
                "-j",
                "MASQUERADE",
            ])
            .output();

        // Add FORWARD rules
        let _ = std::process::Command::new("iptables")
            .args(["-I", "FORWARD", "-s", "10.0.85.0/24", "-j", "ACCEPT"])
            .output();
        let _ = std::process::Command::new("iptables")
            .args(["-I", "FORWARD", "-d", "10.0.85.0/24", "-j", "ACCEPT"])
            .output();
        info!("Setup NAT masquerading and forwarding");
    }

    // Create shared state for hybrid data handler
    let state = Arc::new(RwLock::new(HybridDataState {
        _shared_secret: None,
        tun_device: Some(Arc::new(device)),
    }));

    // Start TCP data listener (for Cloudflare Tunnel)
    let state_for_tcp = state.clone();
    let tcp_task = tokio::spawn(async move {
        if let Err(e) = run_hybrid_data_listener(51821, state_for_tcp).await {
            error!("Hybrid data listener error: {}", e);
        }
    });

    // Connect to relay for signaling
    info!("Connecting to relay for signaling...");
    let relay = P2PRelay::connect(
        &_args.relay,
        &_args.vernam,
        &room_id,
        PeerRole::ExitPeer,
        None,
    )
    .await?;

    info!("✅ Connected to relay as Exit Peer (Hybrid Mode)");
    info!("⏳ Waiting for Client to connect...");
    info!("📡 Data port: localhost:51821 (expose via cloudflared)");

    // Wait for Ctrl+C
    info!("");
    info!("Press Ctrl+C to stop...");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl+C received. Shutting down...");
        }
        _ = tcp_task => {
            error!("TCP listener exited unexpectedly");
        }
    }

    // Cleanup
    relay.close().await?;

    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("iptables")
            .args([
                "-t",
                "nat",
                "-D",
                "POSTROUTING",
                "-s",
                "10.0.85.0/24",
                "-j",
                "MASQUERADE",
            ])
            .output();
    }

    info!("✅ Hybrid Exit Peer stopped.");
    Ok(())
}

/// Run as Faisal Swarm node - P2P mesh with DCUtR hole-punching
///
/// Every node is simultaneously:
/// - Client: Uses network for privacy
/// - Relay: Forwards encrypted traffic for others
/// - Exit: Provides internet access for others (native nodes)
#[cfg(feature = "swarm")]
async fn run_swarm_mode(args: Args, room_id: String) -> Result<(), BoxError> {
    use crate::swarm_controller::{SwarmController, SwarmControllerConfig};

    info!("🌐 Starting Faisal Swarm Mode...");
    info!("   Room: {}", room_id);
    info!("   Signaling: {}", args.relay);
    info!("   Mode: Client + Relay + Exit");

    // Generate random IP if not provided
    let vpn_address = args.vpn_address.unwrap_or_else(|| {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        format!(
            "10.{}.{}.{}",
            rng.gen::<u8>(),
            rng.gen::<u8>(),
            rng.gen::<u8>()
        )
    });
    info!("🎲 Assigned VPN IP: {}", vpn_address);

    // Create swarm configuration from CLI args
    let config = SwarmControllerConfig {
        enable_client: !args.no_client,
        enable_relay: !args.no_relay,
        enable_exit: !args.no_exit, // Enabled by default unless explicitly disabled
        room_id: room_id.clone(),
        relay_url: args.relay.clone(),
        vernam_url: args.vernam.clone(),
        exit_consent_given: args.exit_consent,
        vpn_address,
        server_mode: args.server, // Role-based routing handled by p2p_vpn.rs
    };

    info!("🔧 Configuration:");
    info!("   - VPN Client: {}", config.enable_client);
    info!("   - Relay Service: {}", config.enable_relay);
    info!("   - Exit Service: {}", config.enable_exit);

    if config.enable_exit && !args.exit_consent {
        info!("⚠️  Exit Node Active (Default). You are contributing to the swarm!");
        info!("   Use --no-exit to disable if required.");
    }
    info!("");

    // Create and start swarm controller
    let mut controller = SwarmController::new(config);

    info!("📡 Starting swarm services...");
    info!("Press Ctrl+C to stop...");

    // Run swarm with Ctrl+C handling
    tokio::select! {
        result = controller.start() => {
            if let Err(e) = result {
                error!("Swarm error: {}", e);
                return Err(e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl+C received. Shutting down...");
            controller.stop().await?;
        }
    }

    info!("✅ Faisal Swarm stopped.");
    Ok(())
}
