# syntax=docker/dockerfile:1

# ---- Stage 1: build a static musl binary ------------------------------------
FROM rust:1-alpine AS builder

RUN apk add --no-cache musl-dev gcc

WORKDIR /app

# Build dependencies first so this layer is cached until Cargo.* changes.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src target/release/axum-server target/release/deps/axum_server*

# Build the real application.
COPY src ./src
RUN cargo build --release --locked

# ---- Stage 2: minimal runtime ------------------------------------------------
FROM alpine:3.22 AS runtime

RUN apk add --no-cache ca-certificates \
    && addgroup -S app && adduser -S -G app -H -s /sbin/nologin app

WORKDIR /app

COPY --from=builder /app/target/release/axum-server /usr/local/bin/axum-server
# Handlebars templates are read at startup; the server refuses to start without them.
COPY --chown=app:app templates ./templates
COPY --chown=app:app blog ./blog
COPY --chown=app:app public ./public

USER app

EXPOSE 8000

# BusyBox wget ships with Alpine, so no extra packages are needed.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget -qO /dev/null "http://0.0.0.0:8000/api/v1/health" || exit 1

# Set the entrypoint and default command
ENTRYPOINT ["axum-server"]
CMD ["--host", "0.0.0.0", "--port", "8000", "--templates-dir", "/app/templates", "--blog-dir", "/app/blog", "--static-root", "/app/public"]
