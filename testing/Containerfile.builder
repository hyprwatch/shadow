# Shared builder image containing:
# - Compiled shadow agent binary
# - Downloaded osquery binary
#
# Build this first, then distro images copy from it:
#   docker build -t shadow-builder -f testing/Containerfile.builder .

FROM rust:1.83-slim-bookworm AS rust-builder
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src target/release/shadow target/release/deps/shadow*

# Build actual binary
COPY src ./src
RUN cargo build --release

# Download osquery
FROM debian:12-slim AS osquery-downloader
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fSL https://github.com/osquery/osquery/releases/download/5.20.0/osquery-5.20.0_1.linux_x86_64.tar.gz -o /tmp/osquery.tar.gz \
    && tar -xzf /tmp/osquery.tar.gz -C /tmp \
    && mv /tmp/opt/osquery/bin/osqueryd /osqueryd \
    && chmod +x /osqueryd

# Final builder image with both binaries
FROM debian:12-slim
COPY --from=rust-builder /build/target/release/shadow /shadow
COPY --from=osquery-downloader /osqueryd /osqueryd
