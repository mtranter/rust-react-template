# syntax=docker/dockerfile:1

# ---- Stage 1: build the frontend (SPA embedded into the binary) ----
FROM node:22-slim AS frontend
RUN corepack enable
WORKDIR /app/frontend
# Frontend is added later; this stage no-ops gracefully until it exists.
COPY frontend/package.json frontend/pnpm-lock.yaml* ./
RUN pnpm install --frozen-lockfile
COPY frontend/ ./
RUN pnpm run build

# ---- Stage 2: build the Rust server (embeds frontend/dist) ----
FROM rust:1.97-slim-bookworm AS backend
WORKDIR /app
ENV SQLX_OFFLINE=true
# Cache dependency compilation.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY .sqlx ./.sqlx
# The embedded frontend assets from stage 1.
COPY --from=frontend /app/frontend/dist ./frontend/dist
RUN cargo build --release --bin server --features embed-frontend

# ---- Stage 3: runtime ----
# distroless/cc: glibc, tiny, no shell. Avoids musl/aws-lc-rs toolchain friction.
FROM gcr.io/distroless/cc-debian12 AS runtime
COPY --from=backend /app/target/release/server /usr/local/bin/server
EXPOSE 8080
ENV APP_HOST=0.0.0.0 APP_PORT=8080 APP_ENV=prod
ENTRYPOINT ["/usr/local/bin/server"]
