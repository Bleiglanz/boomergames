# Multi-stage production build for Boomergames (single non-root distroless image, ~30 MB)
# Reference: incubation/plan.md §9.7

# ---- Stage 1: Build WASM + Vite static SPA ----
FROM rust:1.88-bookworm AS web-builder
WORKDIR /src

RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates nodejs npm \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add wasm32-unknown-unknown \
    && cargo install wasm-pack --locked

COPY Cargo.toml Cargo.lock clippy.toml rust-toolchain.toml ./
COPY crates ./crates
COPY migrations ./migrations
COPY web ./web

RUN wasm-pack build crates/boomer-client --target web --release --out-dir ../../web/src/wasm --out-name boomer_client \
    && cd web && npm ci && npm run build

# ---- Stage 2: Build release boomer-server binary ----
FROM rust:1.88-bookworm AS server-builder
WORKDIR /src

COPY Cargo.toml Cargo.lock clippy.toml rust-toolchain.toml ./
COPY crates ./crates
COPY migrations ./migrations

RUN cargo build --release --bin boomer-server --locked \
    && strip /src/target/release/boomer-server

# ---- Stage 3: Minimal non-root distroless runtime ----
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app

COPY --from=server-builder /src/target/release/boomer-server /app/boomer-server
COPY --from=web-builder /src/web/dist /app/web/dist

ENV BOOMER_BIND=0.0.0.0:8080 \
    BOOMER_STATIC_DIR=/app/web/dist \
    BOOMER_LOG_FORMAT=json \
    RUST_LOG=info,boomer_server=info

EXPOSE 8080
USER nonroot:nonroot
ENTRYPOINT ["/app/boomer-server"]
