use clap::{Parser, ValueEnum};

/// Operating mode
#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// SOCKS5 proxy mode (browser only)
    #[value(name = "socks5")]
    Socks5,
    /// HTTP proxy mode (HTTPS via fetch)
    #[value(name = "http")]
    Http,
    /// System-wide VPN mode (requires admin/root)
    #[cfg(feature = "vpn")]
    #[value(name = "vpn")]
    Vpn,
    /// P2P Client mode (connects to Exit Peer)
    #[value(name = "p2p-client")]
    P2pClient,
    /// P2P VPN mode (Triple-Blind Architecture)
    #[cfg(feature = "vpn")]
    #[value(name = "p2p-vpn")]
    P2pVpn,
    /// Exit Peer mode (forward traffic for others)
    #[value(name = "exit-peer")]
    ExitPeer,
    /// Exit Peer VPN mode (Layer 3 Forwarding)
    #[cfg(feature = "vpn")]
    #[value(name = "exit-peer-vpn")]
    ExitPeerVpn,
    /// Entry Node mode (UDP Relay)
    #[value(name = "entry-node")]
    EntryNode,
    /// Exit Node UDP mode (TUN interface)
    #[cfg(feature = "vpn")]
    #[value(name = "exit-node-udp")]
    ExitNodeUdp,
    /// Exit Peer Hybrid mode - Worker signaling + Cloudflare Tunnel data
    #[value(name = "exit-peer-hybrid")]
    ExitPeerHybrid,
    /// Faisal Swarm mode - P2P mesh with DCUtR hole-punching and bandwidth sharing
    #[cfg(feature = "swarm")]
    #[value(name = "swarm")]
    Swarm,
    /// Send file to peer
    #[value(name = "send-file")]
    SendFile,
    /// Receive file from peer
    #[value(name = "receive-file")]
    ReceiveFile,
}

/// ZKS-Tunnel VPN Client
#[derive(Parser, Debug, Clone)]
#[command(name = "zks-vpn")]
#[command(author = "Md Wasif Faisal")]
#[command(version = "0.1.0")]
#[command(about = "Serverless VPN via Cloudflare Workers", long_about = None)]
pub struct Args {
    /// ZKS-Tunnel Worker WebSocket URL
    #[arg(
        short,
        long,
        default_value = "wss://zks-tunnel-relay.md-wasif-faisal.workers.dev"
    )]
    pub worker: String,

    /// Operating mode: socks5 (browser only) or vpn (system-wide)
    #[arg(short, long, value_enum, default_value_t = Mode::Socks5)]
    pub mode: Mode,

    /// Local SOCKS5 proxy port (socks5 mode only)
    #[arg(short, long, default_value_t = 1080)]
    pub port: u16,

    /// Bind address (socks5 mode only)
    #[arg(short, long, default_value = "127.0.0.1")]
    pub bind: String,

    /// TUN device name (vpn mode only)
    #[arg(long, default_value = "zks0")]
    pub tun_name: String,

    /// VPN IP address (auto-generated if not provided)
    #[arg(long)]
    pub vpn_address: Option<String>,

    /// Exit Peer VPN IP address (gateway for routing)
    #[arg(long, default_value = "10.0.85.2")]
    pub exit_peer_address: String,

    /// Enable kill switch - block traffic if VPN disconnects (vpn mode only)
    #[arg(long)]
    pub kill_switch: bool,

    /// Enable DNS leak protection (vpn mode only)
    #[arg(long)]
    pub dns_protection: bool,

    /// Room ID for P2P mode (shared between Client and Exit Peer)
    #[arg(long)]
    pub room: Option<String>,

    /// Relay URL for P2P mode (defaults to zks-tunnel-relay worker)
    #[arg(
        long,
        default_value = "wss://zks-tunnel-relay.md-wasif-faisal.workers.dev"
    )]
    pub relay: String,

    /// ZKS-Vernam key server URL (for double-key encryption)
    #[arg(long, default_value = "https://zks-key.md-wasif-faisal.workers.dev")]
    pub vernam: String,

    /// Constant Rate Padding in Kbps (traffic analysis defense)
    /// Set to 0 to disable. Example: --padding 100 for 100 Kbps padding
    #[arg(long, default_value_t = 0)]
    pub padding: u32,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,

    /// Swarm: Consent to run as exit node (legal requirement)
    #[arg(long)]
    pub exit_consent: bool,

    /// Swarm: Disable relay service (default: enabled)
    #[arg(long)]
    pub no_relay: bool,

    /// Swarm: Disable exit service (default: disabled, requires --exit-consent)
    #[arg(long)]
    pub no_exit: bool,

    /// Swarm: Disable VPN client (default: enabled)
    #[arg(long)]
    pub no_client: bool,

    /// Swarm: Run in Server Mode (Exit Node with NAT, no default route change)
    #[arg(long)]
    pub server: bool,

    /// Upstream SOCKS5 proxy (e.g., 127.0.0.1:9050) to route traffic through
    #[arg(long)]
    pub proxy: Option<String>,

    /// Exit Node address for Entry Node mode (e.g., 213.35.103.204:51820)
    #[arg(long, default_value = "213.35.103.204:51820")]
    pub exit_node: String,

    /// Listen port for Entry Node mode (UDP)
    #[arg(long, default_value_t = 51820)]
    pub listen_port: u16,

    /// File path for transfer (send-file/receive-file mode)
    #[arg(long)]
    pub file: Option<String>,

    /// Destination peer ID (send-file mode)
    #[arg(long)]
    pub dest: Option<String>,

    /// Transfer ticket (receive-file mode)
    #[arg(long)]
    pub ticket: Option<String>,

    /// Run as a Windows Service
    #[arg(long)]
    pub service: bool,

    /// Install as a Windows Service
    #[arg(long)]
    pub install_service: bool,

    /// Uninstall the Windows Service
    #[arg(long)]
    pub uninstall_service: bool,
}
