# syntax=docker/dockerfile:1.7
# dra-server: build stage (Rust) → run stage (distroless, non-root).
#   docker build -t dra-server backend/
# Configuration is CLI/env only (DRA_*), see `dra-server serve --help`.

ARG RUST_VERSION=1.94

# ── Build ────────────────────────────────────────────────────────────────
FROM rust:${RUST_VERSION}-bookworm AS build
WORKDIR /src

# Dependencies first, so source changes don't rebuild them.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && touch src/lib.rs \
    && cargo fetch --locked

COPY src ./src
COPY migrations ./migrations
# Migrations are embedded at compile time (sqlx::migrate!); queries are not checked against a database.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked \
    && install -D target/release/dra-server /out/dra-server

# ── Run ──────────────────────────────────────────────────────────────────
FROM gcr.io/distroless/cc-debian12:nonroot AS run
COPY --from=build /out/dra-server /usr/local/bin/dra-server

# Listen on all interfaces inside the container; JSON logs for log collectors.
ENV DRA_LISTEN_ADDR=0.0.0.0:8090 \
    DRA_LOG_FORMAT=json
EXPOSE 8090
USER nonroot:nonroot

HEALTHCHECK --interval=15s --timeout=6s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/dra-server", "healthcheck", "--addr", "127.0.0.1:8090"]

ENTRYPOINT ["/usr/local/bin/dra-server"]
CMD ["serve"]
