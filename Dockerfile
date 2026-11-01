# syntax=docker/dockerfile:1

# ---- Stage 1: build the frontend (SPA embedded into the binary) ----
FROM node:24-slim AS frontend
RUN corepack enable
WORKDIR /app/frontend
COPY frontend/package.json frontend/pnpm-lock.yaml frontend/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY frontend/ ./
RUN pnpm run generate && pnpm run build

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
RUN cargo build --release --bin server --bin migrate --features embed-frontend
# Non-prod images only: `docker build --build-arg SEED=true`. A separate build so
# the seed feature never unifies into the server binary.
ARG SEED=false
RUN mkdir out && cp target/release/server target/release/migrate out/ \
 && if [ "$SEED" = "true" ]; then \
      cargo build --release -p server --bin seed --features seed && cp target/release/seed out/; \
    fi

# ---- Stage 3: runtime ----
# distroless/cc: glibc, tiny, no shell. Avoids musl/aws-lc-rs toolchain friction.
FROM gcr.io/distroless/cc-debian12 AS runtime
# `migrate` and `seed` run as one-off pre-start tasks from this image, e.g. a
# Kubernetes init container with `command: ["/usr/local/bin/migrate"]`.
COPY --from=backend /app/out/ /usr/local/bin/
EXPOSE 8080
ENV APP_HOST=0.0.0.0 APP_PORT=8080 APP_ENV=prod
ENTRYPOINT ["/usr/local/bin/server"]
