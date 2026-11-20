# One binary serves the API and the embedded SPA

In production the built SPA (`frontend/dist`) is embedded into the server binary with `rust-embed` (poem's `embed` feature, behind `server`'s `embed-frontend` feature), with a catch-all fallback to `index.html` for client-side routing; the embedded endpoint has no fallback of its own. One artifact, no separate static host. In development Vite serves the frontend with HMR on `:5173` and proxies `/api` to the backend on `:8080`, which runs headless; the Generated Client's `baseUrl` is `/api`, so the same code works same-origin in both.

The image is multi-stage: a `node` stage builds the SPA, a `rust` stage builds `server`, `worker` and `migrate` (and `seed` when asked), and the runtime is `gcr.io/distroless/cc`. Glibc rather than musl avoids musl's toolchain friction, and the build needs no system OpenSSL: TLS is rustls throughout. The app's own crypto stays on `ring` (`jsonwebtoken`'s `rust_crypto`, `reqwest` without default TLS, sqlx). The `assistant`'s Bedrock client (Spec 0012) brings the AWS SDK's `default-https-client`, rustls on `aws-lc-rs`, which is accepted: `aws-lc-sys` compiles C (and uses cmake where its build wants it), which the `rust` build image, CI's runners and the dev box all have. With both providers built into rustls, it can't choose one from its features, so `app-bus` installs `ring` as the process's default before redis's TLS needs one.

## Considered options

- **Serving `dist/` from disk** (`StaticFilesEndpoint`, with a free SPA fallback). A second artifact to ship beside the binary.
- **S3/CloudFront static hosting.** A documented future variant, not built.

## Consequences

- A crate whose TLS takes rustls' process default (redis does) needs one installed first while both `ring` and `aws-lc-rs` are in the tree, or it panics on its first TLS connection; the reqwest, sqlx and AWS SDK clients name their provider and need none.

- The backend builds before `frontend/dist` exists only because `embed-frontend` is optional; `make build` and the Dockerfile turn it on.
