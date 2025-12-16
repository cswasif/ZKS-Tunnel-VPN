# 🚀 ZKS-Tunnel VPN - Usage Guide

A **free, serverless VPN** that runs on Cloudflare Workers. Zero operational cost, global deployment, ZKS encryption.

## Quick Start (Users)

### 1. Download the Client

**Option A: Pre-built binaries**
- Go to [Releases](https://github.com/YOUR_USERNAME/ZKS_VPN/releases)
- Download for your OS:
  - Windows: `zks-vpn-windows-x64.zip`
  - macOS: `zks-vpn-macos-x64.tar.gz`
  - Linux: `zks-vpn-linux-x64.tar.gz`

**Option B: Build from source**
```bash
cargo build --release -p zks-tunnel-client
# Binary at: target/release/zks-vpn
```

### 2. Run the Client

```bash
# Connect to a ZKS-Tunnel Worker
./zks-vpn --worker wss://YOUR_WORKER.workers.dev/tunnel

# Or with custom local port
./zks-vpn --worker wss://YOUR_WORKER.workers.dev/tunnel --port 1080
```

### 3. Configure Your Browser/Device

Set SOCKS5 proxy to:
- **Host:** `127.0.0.1`
- **Port:** `1080` (default)

**Firefox:** Settings → Network Settings → Manual proxy → SOCKS5

**Chrome:** Use a proxy extension like SwitchyOmega

**System-wide (macOS):** System Preferences → Network → Advanced → Proxies

---

## Self-Hosting (Deploy Your Own)

### Prerequisites
- [Rust](https://rustup.rs/) installed
- [Cloudflare account](https://dash.cloudflare.com/sign-up) (free tier works!)
- [Wrangler CLI](https://developers.cloudflare.com/workers/wrangler/install-and-update/)

### 1. Clone & Build

```bash
git clone https://github.com/YOUR_USERNAME/ZKS_VPN.git
cd ZKS_VPN
cargo build --release
```

### 2. Configure Cloudflare

```bash
# Login to Cloudflare
wrangler login

# Navigate to worker
cd zks-tunnel-worker

# Edit wrangler.toml if needed
```

### 3. Deploy Worker

```bash
wrangler deploy
# Output: https://zks-tunnel.YOUR_SUBDOMAIN.workers.dev
```

### 4. Share with Friends

Your Worker URL is: `wss://zks-tunnel.YOUR_SUBDOMAIN.workers.dev/tunnel`

Share this with anyone who needs VPN access!

---

## CLI Reference

```
zks-vpn - ZKS-Tunnel VPN Client

USAGE:
    zks-vpn --worker <WORKER_URL>

OPTIONS:
    -w, --worker <URL>    Worker WebSocket URL (required)
                          Example: wss://zks-tunnel.example.workers.dev/tunnel
    
    -p, --port <PORT>     Local SOCKS5 proxy port [default: 1080]
    
    -v, --verbose         Enable verbose logging
    
    -h, --help            Print help
    
    -V, --version         Print version
```

---

## How It Works

```
┌─────────────┐     SOCKS5      ┌─────────────┐     WebSocket     ┌─────────────┐
│   Browser   │ ──────────────► │ zks-vpn     │ ─────────────────► │ CF Worker   │
│             │                 │ (localhost) │    (encrypted)    │ (Edge)      │
└─────────────┘                 └─────────────┘                    └──────┬──────┘
                                                                          │
                                                                          │ TCP
                                                                          ▼
                                                                   ┌─────────────┐
                                                                   │ Destination │
                                                                   │ (google.com)│
                                                                   └─────────────┘
```

1. Your browser connects to `zks-vpn` via SOCKS5
2. `zks-vpn` tunnels traffic over WebSocket to Cloudflare Worker
3. Worker connects to the actual destination
4. All traffic appears to come from Cloudflare's IP

---

## FAQ

**Q: Is this really free?**
A: Yes! Cloudflare Workers free tier includes 100,000 requests/day.

**Q: How is this different from a regular VPN?**
A: No servers to maintain! Workers scale automatically and run on 300+ edge locations.

**Q: Is my traffic encrypted?**
A: Yes. WebSocket over HTTPS (wss://) + ZKS protocol encryption.

**Q: Can I use this for...?**
A: This is for educational purposes. Respect laws and ToS.

---

## Troubleshooting

| Issue | Solution |
|-------|----------|
| Connection refused | Check Worker URL is correct |
| Slow speeds | Normal for free tier; upgrade for higher limits |
| Browser not using proxy | Verify SOCKS5 settings in browser |
| Worker deploy fails | Run `wrangler login` again |

---

## License

MIT License - Free to use, modify, and distribute.
