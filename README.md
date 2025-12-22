# ZKS Protocol & VPN Client

ZKS (Zero-Knowledge Swarm) is a next-generation decentralized network protocol designed for high-security, censorship-resistant communication. This repository contains the reference implementation of the ZKS VPN client, utilizing the **Faisal Swarm** topology and **Wasif Vernam** encryption.

## Core Protocol Architecture

The ZKS protocol implements a unique multi-hop architecture designed to operate in highly restrictive network environments where traditional VPNs (WireGuard, OpenVPN) are blocked.

### 1. Faisal Swarm Topology
The network operates on a swarm-based topology that separates signaling from data transport:

*   **Signaling Layer**: Uses WebSocket over TLS (WSS) to masquerade as standard HTTPS traffic, rendering it resistant to Deep Packet Inspection (DPI).
*   **Data Layer (DCUtR)**: Implements **Direct Connection Upgrade through Relay**. The protocol initially connects via a relay but attempts to upgrade to a direct P2P connection (UDP/QUIC) using NAT hole-punching techniques (libp2p).
*   **Multi-Hop Routing**: Traffic is routed through a dynamic chain of nodes:
    *   **Entry Node**: The client device or a dedicated entry VPS.
    *   **Relay**: A serverless edge component (Cloudflare Workers) for signaling and initial data relay.
    *   **Exit Node**: The final hop that forwards traffic to the internet.

### 2. Wasif Vernam Encryption
ZKS employs a "Double-Key Defense" system known as **Wasif Vernam**, combining post-quantum cryptography with information-theoretic principles:

*   **Base Layer**: **ChaCha20Poly1305** (IETF variant) for high-speed authenticated encryption.
*   **Enhancement Layer**: A secondary **Remote Entropy Stream** is XORed with the plaintext before encryption. This ensures that even if the ChaCha20 key is compromised, the message remains secure without the entropy stream.
*   **Key Exchange**: Authenticated 3-message handshake using a hybrid scheme:
    *   **Post-Quantum**: **ML-KEM-768** (Kyber) for protection against "Harvest Now, Decrypt Later" attacks.
    *   **Classical**: **X25519** for established security guarantees.
    *   **Identity**: Ephemeral identity keys are derived from the `room_id`, ensuring mutual authentication without central PKI.

## System Components

### `zks-tunnel-client`
The primary user application written in Rust.
*   **System-Wide VPN**: Creates a **TUN device** (`zks0`) and uses a userspace TCP/IP stack (`netstack-smoltcp`) to route all OS traffic.
*   **Kill Switch**: Integrated OS-level kill switch (Windows `netsh` / Linux `iptables`) prevents data leaks if the tunnel drops.
*   **SOCKS5 Proxy**: Lightweight mode for application-specific tunneling.
*   **File Transfer**: Secure, resumable P2P file transfer with **Transfer Tickets**.

### `zks-tunnel-worker`
A serverless relay running on Cloudflare Workers. It handles the initial signaling and WebSocket relaying, ensuring high availability and zero infrastructure maintenance.

### `zks-tunnel-proto`
Defines the binary wire format, including:
*   `Connect` / `Data` / `Close` frames for stream management.
*   `IpPacket` (0x20) for raw layer-3 VPN traffic.
*   `UdpDatagram` (0x07) for stateless UDP.

## Features

*   **Zero-Knowledge Architecture**: The relay (Cloudflare) cannot decrypt traffic; it only sees encrypted WebSocket frames.
*   **Resumable File Transfer**: Built-in P2P file sharing using `zks://` tickets.
*   **DNS Leak Protection**: Intercepts UDP port 53 traffic and resolves via DoH (DNS over HTTPS) through the tunnel.
*   **Cross-Platform**: Windows, Linux, macOS.

## Usage

### Installation
```bash
cargo build --release
```

### Modes

**1. System-Wide VPN (Recommended)**
Routes all traffic through the secure tunnel. Requires Administrator/Root privileges.
```bash
# Linux/macOS
sudo ./target/release/zks-vpn --mode vpn --worker wss://your-relay.workers.dev/tunnel

# Windows (Admin PowerShell)
./target/release/zks-vpn.exe --mode vpn --worker wss://your-relay.workers.dev/tunnel
```

**2. SOCKS5 Proxy**
Starts a local proxy on port 1080.
```bash
./target/release/zks-vpn --mode socks5 --worker wss://your-relay.workers.dev/tunnel
```

**3. Secure File Transfer**
*Sender:*
```bash
./target/release/zks-vpn --mode send-file --file ./secret.pdf
# Generates a ticket: zks://...
```
*Receiver:*
```bash
./target/release/zks-vpn --mode receive-file --ticket zks://...
```

## Development

The project is a Rust workspace:
*   `zks-tunnel-client`: Client implementation.
*   `zks-tunnel-proto`: Protocol library.
*   `zks-tunnel-worker`: Relay implementation.

Run tests:
```bash
cargo test --workspace
```

## License
GNU Affero General Public License v3.0 (AGPL-3.0)
