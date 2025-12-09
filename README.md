# Shadow

Shadow is the lightweight agent for [Hyprwatch](https://hyprwatch.cloud) - a fleet management and security observability platform powered by osquery.

Shadow automatically downloads and manages osquery, handles enrollment, and maintains a persistent connection to the Hyprwatch server for real-time distributed queries.

## Features

- **Zero-dependency installation** - Single binary, no runtime dependencies
- **Automatic osquery provisioning** - Downloads and manages osquery automatically
- **Cross-platform** - Linux (x86_64, aarch64) and macOS (Intel, Apple Silicon)
- **Secure enrollment** - TLS-based communication with the Hyprwatch server
- **Lightweight** - Minimal resource footprint

## Quick Install

Get your organization token from the [Hyprwatch dashboard](https://hyprwatch.cloud), then run:

```bash
curl -sSL https://hyprwatch.cloud/install/YOUR_ORG_TOKEN | sudo sh
```

### macOS

```bash
curl -sSL https://hyprwatch.cloud/install/YOUR_ORG_TOKEN | sh
```

> Note: On macOS, you can run without `sudo` for a user-level installation.

### Verify Installation

**Linux:**
```bash
systemctl status shadow
journalctl -u shadow -f
```

**macOS:**
```bash
tail -f ~/Library/Application\ Support/shadow/shadow.log
```

## Manual Installation

### Download Binary

Download the appropriate binary for your platform from [GitHub Releases](https://github.com/hyprwatch/shadow/releases):

| Platform | Architecture | Binary |
|----------|--------------|--------|
| Linux | x86_64 | `shadow-linux-x86_64` |
| Linux | aarch64/arm64 | `shadow-linux-aarch64` |
| macOS | Intel | `shadow-darwin-x86_64` |
| macOS | Apple Silicon | `shadow-darwin-aarch64` |

### Run Manually

```bash
chmod +x shadow-linux-x86_64
./shadow-linux-x86_64 --org-token YOUR_ORG_TOKEN --server hyprwatch.cloud
```

### Command Line Options

```
Usage: shadow [OPTIONS] --org-token <ORG_TOKEN>

Options:
  -t, --org-token <ORG_TOKEN>      Organization token for enrollment [env: SHADOW_ORG_TOKEN]
  -s, --server <SERVER>            Server hostname [env: SHADOW_SERVER_HOST] [default: hyprwatch.cloud]
  -d, --data-dir <DATA_DIR>        Data directory for osquery database and logs [env: SHADOW_DATA_DIR]
  -o, --osqueryd-path <PATH>       Path to osqueryd binary (skips auto-download)
  -v, --verbose                    Enable verbose logging [env: SHADOW_VERBOSE]
      --host-identifier <MODE>     Host identifier mode: uuid or instance [default: uuid]
      --distributed-interval <N>   Distributed query polling interval in seconds [default: 10]
  -h, --help                       Print help
  -V, --version                    Print version
```

## Upgrade

```bash
curl -sSL https://hyprwatch.cloud/install/YOUR_ORG_TOKEN | sudo sh -s -- upgrade
```

Or pin to a specific version:

```bash
VERSION=0.2.0 curl -sSL https://hyprwatch.cloud/install/YOUR_ORG_TOKEN | sudo sh -s -- upgrade
```

## Uninstall

```bash
curl -sSL https://hyprwatch.cloud/install/YOUR_ORG_TOKEN | sudo sh -s -- uninstall
```

Or manually:

**Linux:**
```bash
sudo systemctl stop shadow
sudo systemctl disable shadow
sudo rm /etc/systemd/system/shadow.service
sudo rm /usr/local/bin/shadow
sudo rm -rf /var/lib/shadow  # Optional: remove data
```

**macOS:**
```bash
launchctl unload ~/Library/LaunchAgents/cloud.hyprwatch.shadow.plist
rm ~/Library/LaunchAgents/cloud.hyprwatch.shadow.plist
rm ~/bin/shadow  # or /usr/local/bin/shadow if installed with sudo
rm -rf ~/Library/Application\ Support/shadow  # Optional: remove data
```

## Building from Source

### Prerequisites

- Rust 1.70 or later
- For cross-compilation: [cross](https://github.com/cross-rs/cross)

### Build

```bash
# Native build
cargo build --release

# Cross-compile for all platforms
./scripts/build-release.sh all
```

### Output

Binaries are output to `target/releases/`:
- `shadow-linux-x86_64`
- `shadow-linux-aarch64`
- `shadow-darwin-x86_64`
- `shadow-darwin-aarch64`

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Shadow Agent                             │
├─────────────────────────────────────────────────────────────────┤
│  1. Enrollment                                                   │
│     POST /api/shadow/enroll {host_id, org_token}                │
│     → Returns enroll_secret                                      │
│                                                                  │
│  2. osquery Provisioning                                         │
│     Downloads osquery from GitHub releases if not present        │
│     Verifies SHA256 checksum                                     │
│                                                                  │
│  3. osquery TLS Mode                                             │
│     Starts osqueryd with TLS endpoints pointing to Hyprwatch:   │
│     - /api/osquery/enroll                                        │
│     - /api/osquery/config                                        │
│     - /api/osquery/log                                           │
│     - /api/osquery/distributed/read                              │
│     - /api/osquery/distributed/write                             │
└─────────────────────────────────────────────────────────────────┘
```

## Troubleshooting

### Agent not appearing in dashboard

1. Check if the service is running:
   ```bash
   # Linux
   systemctl status shadow
   
   # macOS
   launchctl list | grep shadow
   ```

2. Check logs for errors:
   ```bash
   # Linux
   journalctl -u shadow -f
   
   # macOS
   tail -f ~/Library/Application\ Support/shadow/shadow.log
   ```

3. Verify network connectivity:
   ```bash
   curl -I https://hyprwatch.cloud/api/shadow/enroll
   ```

### osquery download fails

If automatic osquery download fails, you can install osquery manually and point shadow to it:

```bash
# Install osquery (example for Ubuntu/Debian)
curl -L https://pkg.osquery.io/deb/osquery.gpg | sudo apt-key add -
echo "deb [arch=amd64] https://pkg.osquery.io/deb deb main" | sudo tee /etc/apt/sources.list.d/osquery.list
sudo apt update && sudo apt install osquery

# Run shadow with system osquery
shadow --org-token YOUR_TOKEN --osqueryd-path /usr/bin/osqueryd
```

### Firewall issues

Shadow requires outbound HTTPS (port 443) access to:
- `hyprwatch.cloud` (or your self-hosted server)
- `github.com` (for osquery downloads)

## License

[MIT License](LICENSE)

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.
