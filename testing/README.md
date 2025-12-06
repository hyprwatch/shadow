# Shadow Testing Environment

Docker-based multi-distro testing environment for simulating multiple shadow agents.

## Prerequisites

- Docker with Compose V2
- Hyprwatch server running on host at `https://localhost:4001`

## Quick Start

```bash
cd shadow

# Step 1: Build the shared builder image (compiles shadow + downloads osquery)
docker build -t shadow-builder -f testing/Containerfile.builder .

# Step 2: Build and start all shadows (2 of each distro = 8 total)
cd testing
docker compose up -d

# View logs
docker compose logs -f

# Stop all
docker compose down
```

## Scaling

```bash
# Start specific number of each distro
docker compose up -d --scale ubuntu=3 --scale fedora=2 --scale debian=2 --scale rocky=1

# Scale a running service
docker compose up -d --scale ubuntu=5
```

## Distros Included

| Service | Distribution     | Base Image             |
|---------|------------------|------------------------|
| ubuntu  | Ubuntu 24.04 LTS | ubuntu:24.04           |
| fedora  | Fedora 41        | fedora:41              |
| debian  | Debian 12        | debian:12-slim         |
| rocky   | Rocky Linux 9    | rockylinux:9-minimal   |

Note: Alpine is not supported because osquery requires glibc.

## Configuration

### Environment Variables

| Variable             | Default                                  | Description            |
|----------------------|------------------------------------------|------------------------|
| `SHADOW_ORG_TOKEN`   | `ORG_vdGTE2GRb0RU_t15W37EclpwkIh_N2zP`   | Organization token     |
| `SHADOW_SERVER_HOST` | `localhost:4001`                         | Server hostname:port   |
| `SHADOW_CA_CERT`     | `/certs/selfsigned.pem`                  | Path to CA certificate |

### Custom Org Token

```bash
SHADOW_ORG_TOKEN=your_token_here docker compose up -d
```

## Build Architecture

The setup uses a two-phase build for efficiency:

### Phase 1: Builder Image (`shadow-builder`)

Built once with:
```bash
docker build -t shadow-builder -f testing/Containerfile.builder .
```

Contains:
- Compiled shadow agent binary (~6MB)
- Downloaded osquery binary (~274MB)

This image is cached and reused by all distro images.

### Phase 2: Distro Images

Each distro image simply copies the binaries from `shadow-builder`:
```dockerfile
FROM shadow-builder AS builder
FROM ubuntu:24.04
COPY --from=builder /shadow /usr/local/bin/shadow
COPY --from=builder /osqueryd /usr/local/bin/osqueryd
```

### Rebuild After Code Changes

If you modify the shadow agent source code:
```bash
# Rebuild the builder image
cd shadow
docker build -t shadow-builder -f testing/Containerfile.builder .

# Rebuild distro images
cd testing
docker compose build
docker compose up -d
```

## Networking

Uses `network_mode: host` for direct access to the host's network. This means:
- Containers can reach `localhost:4001` directly
- No port mapping needed
- Simpler firewall configuration

## Troubleshooting

### Phoenix server needs restart

If you see HTML error pages in logs mentioning "config changed", restart your Phoenix server:
```bash
cd hyprwatch
mix phx.server
```

### Certificate errors

The compose file mounts the dev certificate from `hyprwatch/priv/cert/selfsigned.pem`. Ensure this file exists.

### View individual container logs

```bash
docker compose logs ubuntu
docker compose logs -f fedora
```

### Check if binaries are in image

```bash
docker run --rm --entrypoint="" testing-ubuntu ls -la /usr/local/bin/
```

## Architecture Notes

- Each container gets a unique UUID from osquery (based on container filesystem)
- Shadows are ephemeral - data is lost on container removal
- osquery v5.20.0 is baked into the builder image
