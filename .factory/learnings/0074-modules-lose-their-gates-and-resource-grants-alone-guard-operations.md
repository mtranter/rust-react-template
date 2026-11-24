# Learnings: 0074, Modules lose their gates

- **Adding a `query!` when `.sqlx` lacks it is chicken-and-egg**: `migrate` won't build offline, and `sqlx prepare` needs the new migration applied. What worked: stub the new repository methods with `todo!()`, build and run `target/debug/migrate` against a scratch database (it applies the migrations, then panics in the stub), restore the file, then `scripts/migrate-unregistered.sh` and `SQLX_OFFLINE=false cargo bin sqlx prepare --workspace -- --all-targets` with that `DATABASE_URL`.
- **The Postgres on :5432 may be another worktree's compose project.** Don't migrate its shared `app` database (another worktree's `migrate` then meets a migration it lacks): `CREATE DATABASE app_<ticket>` and point `DATABASE_URL` there. `scripts/migrate-schema.sh` runs `docker compose exec postgres psql`, which fails when this worktree's compose has no postgres; a `docker` shim on `PATH` that runs plain `psql` for `compose exec` gets round it.
- `cargo clippy --all-features` fails on `server` without `frontend/dist` (RustEmbed); plain `--all-targets` is what `make check` lints.
- **Every Principal holds `user`**. Any rule that reasons per role ("did this role hold X?") must also count `user`'s grants, or it misjudges what the role's holders could do.
- `access`' test driver can stand in an old database with raw SQL (`had_gates`): the API now refuses `module:*` grants, so they can't be made through it.
- `composed_app.rs` was required to stay unchanged; it still says "its module's `Caller`" in a message, which reads fine for `app_web::Caller`.
