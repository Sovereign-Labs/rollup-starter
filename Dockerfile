# syntax=docker/dockerfile:1.7
FROM rust:1.93-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    clang \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --locked --release --bin rollup && \
    cp target/release/rollup /rollup

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /rollup
COPY --from=builder /rollup /usr/local/bin/rollup
COPY configs/celestia/genesis.json /rollup/configs/celestia/genesis.json
COPY configs/celestia/evm_pinned_cache.json /rollup/configs/celestia/evm_pinned_cache.json
VOLUME ["/rollup/rollup-state"]
EXPOSE 12346
ENTRYPOINT ["/usr/local/bin/rollup"]
