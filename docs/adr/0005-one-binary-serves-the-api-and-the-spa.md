# One binary serves the API and the embedded SPA

In production the built SPA (`frontend/dist`) is embedded into the server binary with `rust-embed` (poem's `embed` feature, behind `server`'s `embed-frontend` feature), with a catch-all fallback to `index.html` for client-side routing; the embedded endpoint has no fallback of its own. One artifact, no separate static host. In development Vite serves the frontend with HMR on `:5173` and proxies `/api` to the backend on `:8080`, which runs headless; the Generated Client's `baseUrl` is `/api`, so the same code works same-origin in both.

The image is multi-stage: a `node` stage builds the SPA, a `rust` stage builds `server`, `worker` and `migrate` (and `seed` when asked), and the runtime is `gcr.io/distroless/cc`. Glibc rather than musl avoids musl's toolchain friction, and crypto stays on `ring` (`jsonwebtoken`'s `rust_crypto`, `reqwest` without default TLS) so the build needs no system OpenSSL, `aws-lc-rs` or cmake.

## Considered options

- **Serving `dist/` from disk** (`StaticFilesEndpoint`, with a free SPA fallback). A second artifact to ship beside the binary.
- **S3/CloudFront static hosting.** A documented future variant, not built.

## Consequences

- The backend builds before `frontend/dist` exists only because `embed-frontend` is optional; `make build` and the Dockerfile turn it on.
