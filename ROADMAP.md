# ZKS-VPN Future Roadmap

## 1. ZKS Triple-Blind Architecture (Priority #1)
**Goal**: The "Ultimate Security Model" where no single node knows the full path.
**Status**: **Feasible & High Priority**.

### Why Rust makes this "Blazing Fast":
- **Zero-Cost Abstractions**: We can swap "TCP Socket" for "ZKS Socket" with 0% CPU overhead.
- **XOR Encryption**: The Vernam cipher is the fastest encryption possible (faster than AES).
- **Async I/O**: Rust's Tokio engine handles thousands of concurrent chains without slowing down.

### Architecture
```
User -> [Relay] -> VPS 1 -> [Relay] -> VPS 2 -> Internet
```
- **VPS 1**: Acts as an Exit for User, but a Client for VPS 2.
- **No Bottleneck**: Cloudflare scales infinitely. VPS 1 and VPS 2 use full datacenter bandwidth.

### Implementation Plan
- [ ] Refactor `exit-peer` to support "Upstream ZKS Proxy".
- [ ] Add `--chain-to <room-id>` flag.

## 2. UDP Hole Punching (Direct P2P)
**Goal**: Bypass the Cloudflare Relay for maximum speed.
- Use the Relay only for signaling (exchanging IPs/Keys).
- Establish a direct UDP connection between Client and Exit Peer.
- **Benefit**: Zero relay latency, unlimited bandwidth (limited only by peers).

## 3. Public Swarm Discovery
**Goal**: Allow users to share bandwidth anonymously.
- **DHT (Distributed Hash Table)**: Store active Room IDs.
- **Incentives**: Earn credits for running an Exit Peer (ZKS Token?).
- **Reputation System**: Verify honest peers.

## 4. Obfuscation (Stealth Mode)
**Goal**: Hide ZKS traffic from Deep Packet Inspection (DPI).
- Wrap WebSocket traffic in "fake" HTML or video stream headers.
- Make VPN traffic look like watching YouTube.

## 5. System-Wide Routing (TUN Interface)
**Goal**: Finish the `vpn` mode integration with `tun-rs`.
- Currently, `p2p-client` provides SOCKS5.
- Next step: Connect SOCKS5 to a virtual network card so *all* apps work without configuration.
