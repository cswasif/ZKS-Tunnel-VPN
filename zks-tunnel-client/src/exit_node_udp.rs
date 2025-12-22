//! Exit Node UDP Mode for Multi-Hop VPN
//!
//! Second hop in the Faisal Swarm multi-hop topology.
//! Accepts UDP connections from Entry Node (VPS1) and forwards to Internet.
//!
//! Architecture:
//! ```
//! [Client] <--UDP--> [Entry Node VPS1] <--UDP--> [Exit Node VPS2] <---> [Internet]
//!                    (0.0.0.0:51820)             (0.0.0.0:51820)
//! ```
//!
//! Usage:
//!   sudo zks-vpn --mode exit-node-udp --listen-port 51820

#[cfg(feature = "vpn")]
use std::net::SocketAddr;
#[cfg(feature = "vpn")]
use std::sync::Arc;
#[cfg(feature = "vpn")]
use tokio::net::UdpSocket;
#[cfg(feature = "vpn")]
use tokio::sync::RwLock;
#[cfg(feature = "vpn")]
use tracing::{debug, error, info};

/// Run as Exit Node in UDP mode (Multi-Hop - Second Hop)
///
/// Instead of WebSocket from relay, accepts UDP connections from Entry Node (VPS1).
/// Creates TUN device and forwards IP packets to/from Internet.
///
/// # Arguments
/// * `listen_port` - UDP port to listen on (default: 51820)
#[cfg(feature = "vpn")]
pub async fn run_exit_node_udp(
    listen_port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║      ZKS Exit Node UDP - Faisal Swarm Second Hop             ║");
    info!("╠══════════════════════════════════════════════════════════════╣");
    info!("║  Listen: 0.0.0.0:{:<47} ║", listen_port);
    info!("║  Mode:   Accept UDP from Entry Node                          ║");
    info!("╚══════════════════════════════════════════════════════════════╝");

    // Create TUN device for packet forwarding (10.0.85.2 for exit node)
    info!("Creating TUN device for VPN forwarding...");

    let device = tun_rs::DeviceBuilder::new()
        .ipv4(std::net::Ipv4Addr::new(10, 0, 85, 2), 24, None)
        .mtu(1400)
        .build_async()?;

    info!("✅ TUN device created (10.0.85.2/24)");

    // Enable IP forwarding and NAT on Linux
    #[cfg(target_os = "linux")]
    {
        // Enable IP forwarding
        let forward_result = std::process::Command::new("sysctl")
            .args(["-w", "net.ipv4.ip_forward=1"])
            .output();
        match forward_result {
            Ok(_) => info!("✅ IP forwarding enabled"),
            Err(e) => error!("Failed to enable IP forwarding: {}", e),
        }

        // Setup NAT with MASQUERADE
        let nat_result = std::process::Command::new("iptables")
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
        match nat_result {
            Ok(_) => info!("✅ NAT configured for 10.0.85.0/24"),
            Err(e) => error!("Failed to configure NAT: {}", e),
        }
    }

    #[cfg(target_os = "windows")]
    {
        info!("⚠️ Windows: Manual NAT/ICS configuration may be required");
    }

    // Bind UDP socket
    let bind_addr = format!("0.0.0.0:{}", listen_port);
    let socket = Arc::new(UdpSocket::bind(&bind_addr).await?);
    info!("✅ UDP socket bound to {}", bind_addr);

    // Track Entry Node address (set when we receive first packet)
    let entry_node_addr: Arc<RwLock<Option<SocketAddr>>> = Arc::new(RwLock::new(None));

    info!("⏳ Waiting for Entry Node connection...");

    // Clone Arc references for tasks
    let device = Arc::new(device);
    let device_reader = device.clone();
    let device_writer = device.clone();
    let socket_tx = socket.clone();
    let socket_rx = socket.clone();
    let entry_addr_tx = entry_node_addr.clone();
    let entry_addr_rx = entry_node_addr.clone();

    // Task: TUN → UDP (Internet responses → back to Entry Node)
    let tun_to_udp = tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];

        loop {
            match device_reader.recv(&mut buf).await {
                Ok(n) => {
                    // Read packet from TUN (response from Internet)
                    let packet = &buf[..n];

                    // Send to Entry Node if connected
                    let addr_lock = entry_addr_tx.read().await;
                    if let Some(addr) = *addr_lock {
                        if let Err(e) = socket_tx.send_to(packet, addr).await {
                            error!("Failed to send to Entry Node: {}", e);
                        } else {
                            debug!("← Internet → Entry: {} bytes", n);
                        }
                    }
                }
                Err(e) => {
                    error!("TUN read error: {}", e);
                    break;
                }
            }
        }
    });

    // Task: UDP → TUN (Entry Node packets → to Internet)
    let udp_to_tun = tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];

        loop {
            match socket_rx.recv_from(&mut buf).await {
                Ok((n, addr)) => {
                    // First packet from Entry Node - remember address
                    {
                        let mut addr_lock = entry_addr_rx.write().await;
                        if addr_lock.is_none() {
                            info!("✅ Entry Node connected: {}", addr);
                            *addr_lock = Some(addr);
                        }
                    }

                    // Forward packet to TUN (to Internet via NAT)
                    let packet = &buf[..n];
                    if let Err(e) = device_writer.send(packet).await {
                        error!("TUN write error: {}", e);
                    } else {
                        debug!("→ Entry → Internet: {} bytes", n);
                    }
                }
                Err(e) => {
                    error!("UDP recv error: {}", e);
                    break;
                }
            }
        }
    });

    // Wait for either task to complete (or error)
    tokio::select! {
        _ = tun_to_udp => {
            error!("TUN to UDP task ended");
        }
        _ = udp_to_tun => {
            error!("UDP to TUN task ended");
        }
    }

    Ok(())
}

// Stub when vpn feature is not enabled
#[cfg(not(feature = "vpn"))]
pub async fn run_exit_node_udp(
    _listen_port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Err("Exit Node UDP mode requires VPN feature. Build with: cargo build --features vpn".into())
}
