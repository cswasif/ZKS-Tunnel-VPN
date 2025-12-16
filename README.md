# ZKS-Tunnel VPN

**The World's First Serverless, Free, Zero-Knowledge VPN**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## 🚀 What is ZKS-Tunnel?

ZKS-Tunnel is a revolutionary VPN that runs entirely on **Cloudflare Workers** (free tier) with **Zero-Knowledge Security** encryption. No servers to rent, no monthly fees, no trust required.

```
┌─────────────────────────────────────────────────────────────────┐
│  YOUR PC          CLOUDFLARE EDGE           INTERNET            │
│                   (300+ cities)                                 │
│  ┌─────────┐     ┌─────────────┐     ┌─────────────────────┐   │
│  │ zks-vpn │ ───►│ ZKS Worker  │ ───►│ Any Website/Server  │   │
│  │ :1080   │     │ (Free!)     │     │ Sees Cloudflare IP  │   │
│  └─────────┘     └─────────────┘     └─────────────────────┘   │
│                                                                 │
│  Your IP is HIDDEN. Your data is ENCRYPTED (unbreakable OTP).  │
└─────────────────────────────────────────────────────────────────┘
```

## ✨ Features

- **$0/month** - Runs on Cloudflare Workers free tier
- **Mathematically Unbreakable** - ZKS double-key Vernam cipher (OTP)
- **Global** - 300+ edge locations for low latency
- **Privacy** - Even Cloudflare can't read your traffic (ZKS inner layer)
- **Open Source** - Fully auditable code

## 📦 Project Structure

```
ZKS_VPN/
├── zks-tunnel-proto/    # Shared protocol definitions
├── zks-tunnel-worker/   # Cloudflare Worker (Gateway)
└── zks-tunnel-client/   # Local SOCKS5 client
```

## 🛠️ Quick Start

### 1. Deploy the Worker

```bash
cd zks-tunnel-worker
wrangler deploy
```

### 2. Run the Client

```bash
cd zks-tunnel-client
cargo build --release
./target/release/zks-vpn --worker wss://your-worker.workers.dev/tunnel
```

### 3. Configure Your Browser

Set SOCKS5 proxy to: `127.0.0.1:1080`

**That's it!** All your traffic is now encrypted and tunneled.

## 📖 How It Works

1. **Client** creates a local SOCKS5 proxy (like Tor)
2. **Browser** sends traffic to the proxy
3. **Client** encrypts with ZKS and sends via WebSocket to Worker
4. **Worker** decrypts, opens TCP connection to destination
5. **Response** flows back the same way

## 🔐 Security

ZKS-Tunnel uses a unique **double-key Vernam cipher**:

```
ciphertext = plaintext ⊕ key_A ⊕ key_B
```

- **key_A**: Local CSPRNG (browser's crypto.getRandomValues)
- **key_B**: Remote LavaRand (hardware entropy from lava lamps)

This provides **information-theoretic security** - unbreakable even by quantum computers.

## 📄 License

MIT License - Free to use, modify, and distribute.

## 🙏 Credits

Created by **Md Wasif Faisal** as part of the ZKS (Zero-Knowledge Security) project.

---

**Give the world the best thing ever, for free.** 🌍
