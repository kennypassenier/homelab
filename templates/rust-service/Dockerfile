# Minimal image for a Rust service managed by the homelab (G9).
# Drop this in the root of your Rust repo next to Cargo.toml.
# Replace MYSERVICE with your binary name (the [[bin]] / package name).

FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked

# distroless/cc: glibc + certs, nothing else (~20 MB total). No shell, no
# package manager — nothing to patch, nothing to exploit.
FROM gcr.io/distroless/cc-debian12
COPY --from=build /src/target/release/MYSERVICE /usr/local/bin/MYSERVICE
ENTRYPOINT ["/usr/local/bin/MYSERVICE"]
