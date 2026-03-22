# ── Builder ──────────────────────────────────────────────────────────────────
FROM rust:1.86-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        libclang-dev llvm-dev cmake pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src          src/
COPY aura-core    aura-core/
COPY aura-store   aura-store/
COPY aura-executor aura-executor/
COPY aura-tools   aura-tools/
COPY aura-reasoner aura-reasoner/
COPY aura-kernel  aura-kernel/
COPY aura-swarm   aura-swarm/
COPY aura-terminal aura-terminal/
COPY aura-cli     aura-cli/
COPY aura-agent   aura-agent/

RUN cargo build --release --bin aura \
    && strip target/release/aura

# ── Runtime ─────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        libssl3 ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -g 1000 aura \
    && useradd -u 1000 -g aura -m aura \
    && mkdir -p /data && chown aura:aura /data

COPY --from=builder /build/target/release/aura /usr/local/bin/aura

ENV AURA_LISTEN_ADDR=0.0.0.0:8080 \
    AURA_DATA_DIR=/data \
    RUST_LOG=info

EXPOSE 8080

HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

USER aura

ENTRYPOINT ["aura", "--ui", "none"]
