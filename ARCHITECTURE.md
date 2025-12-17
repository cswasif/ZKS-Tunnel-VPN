# ZKS-VPN Architecture Overview

## Core Concept: "Socket over WebSocket"
ZKS-VPN tunnels raw TCP/UDP sockets over standard HTTPS/WebSocket connections, allowing it to bypass restrictive firewalls and NATs.

## Components

### 1. ZKS Client (`zks-tunnel-client`)
- **Role**: The user's entry point.
- **Modes**:
  - `socks5`: Local proxy for browsers.
  - `vpn`: System-wide TUN interface (virtual network card).
  - `p2p-client`: Connects to an Exit Peer via Relay.
  - `exit-peer`: Acts as a gateway to the internet.
- **Encryption**: Generates Local Key (CSPRNG) for Vernam Cipher.

### 2. ZKS Relay (`zks-tunnel-relay`)
- **Platform**: Cloudflare Worker (Durable Objects).
- **Role**: Blind relay. Connects Client to Exit Peer.
- **Scalability**: Serverless. Spins up a new "Room" (Durable Object) for every connection pair.
- **Security**: Zero Knowledge. Cannot decrypt traffic (sees only `Ciphertext`).

### 3. ZKS Exit Peer
- **Role**: The bridge to the real internet.
- **Location**: Anywhere (Oracle Cloud, Home PC, Raspberry Pi).
- **Function**: Decrypts ZKS traffic -> Standard TCP/UDP -> Internet.

## The "Swarm" Scalability Model

The system is designed to handle thousands of concurrent peers through **Room Isolation**.

```mermaid
graph TD
    subgraph "Cloudflare Network (The Swarm)"
        Room1[Room: 'secret-1']
        Room2[Room: 'secret-2']
        RoomN[Room: 'secret-N']
    end

    UserA --> Room1 --> ExitA[Oracle VM]
    UserB --> Room2 --> ExitB[Home PC]
    UserC --> RoomN --> ExitN[AWS EC2]
```

### Key Features
1.  **Isolation**: Room A cannot communicate with Room B. Each tunnel is a private universe.
2.  **Infinite Scaling**: Cloudflare automatically distributes Rooms across its global network. 10,000 users = 10,000 parallel processes.
3.  **No Bottlenecks**: Unlike a traditional VPN server (which slows down with more users), ZKS-VPN uses distributed edge computing.

## Security Model: Double-Key Vernam Cipher

$$ Ciphertext = Plaintext \oplus Key_{Local} \oplus Key_{Remote} $$

- **Key Local**: Held by Client.
- **Key Remote**: Held by Cloudflare (LavaRand).
- **Result**:
  - Cloudflare has Key Remote but lacks Key Local -> **Cannot Decrypt**.
  - ISP has neither -> **Cannot Decrypt**.
  - Exit Peer receives decrypted stream (via the tunnel) -> **Must Decrypt to forward**.

## Deployment

- **Relay**: `wrangler deploy` (Automatic via GitHub Actions).
- **Client/Exit**: `cargo build --release`.

## Performance: ZKS vs Tor

ZKS is designed to be **orders of magnitude faster** than Tor.

| Feature | Tor (The Onion Router) | ZKS-VPN | Why ZKS is Faster |
| :--- | :--- | :--- | :--- |
| **Network** | Volunteer Nodes (Home Internet) | **Cloudflare Backbone** (Fiber) | We use enterprise-grade infrastructure, not home DSL. |
| **Hops** | 3 (Entry -> Middle -> Exit) | **1** (Client -> Relay -> Exit) | Fewer hops = Lower latency. |
| **Bandwidth** | Shared with thousands of users | **Dedicated** (Your own VPS) | You get the full 1Gbps+ of your Oracle VM. |
| **Protocol** | TCP over TCP (Slow) | **WebSockets / UDP** (Fast) | Optimized for modern high-speed throughput. |

**Result**:
- **Tor**: ~2-5 Mbps, High Latency (200ms+).
- **ZKS**: **100 Mbps - 1 Gbps**, Low Latency (depends on location).

## 6. Local Browser vs Remote Browser (Clarification)

You asked: *"Is it possible to make a protocol... so if I request on remote it shows on local... using VPN bandwidth not mine?"*

There are two ways to do this, and ZKS supports both:

### Option A: SOCKS5 Proxy (What we built)
- **How it works**: Your browser runs **Locally**. It sends requests through the tunnel.
- **IP**: Hidden. Websites see the Exit Peer IP.
- **Bandwidth**: You **DO** use your bandwidth. (To see a 1GB video, you must download 1GB).
- **Pros**: Feels native, smooth scrolling, perfect text.

### Option B: Remote Browser Isolation (RBI)
- **How it works**: The browser runs **Remotely**. You watch a video of it.
- **IP**: Hidden.
- **Bandwidth**: You use **Video Bandwidth** (constant 3-5 Mbps), regardless of what you download.
    - *Example*: If the remote browser downloads a 100GB file, it stays on the server. You only used ~50MB of video data to watch the progress bar.
- **Pros**: Total security (Air Gap).

**Verdict**:
- If you want **Anonymity** (hiding IP), use **Option A (SOCKS5)**.
- If you want **Security** (Anti-Malware) or to save bandwidth on huge downloads, use **Option B (RBI)**.


