FROM rust:1.82-bookworm AS builder
WORKDIR /app

COPY lxc-daemon ./lxc-daemon
RUN cargo build --release --manifest-path lxc-daemon/Cargo.toml

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git openssh-client docker.io \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/lxc-daemon/target/release/LXC /usr/local/bin/LXC
RUN chmod +x /usr/local/bin/LXC

ENTRYPOINT ["/usr/local/bin/LXC"]
