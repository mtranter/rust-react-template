# Learnings: 0077, Modules live in Capability folders

- **A Module learns its Capability through its policies**: `Policies::builder(..).capability(policy_file!("../../capability.yaml"))` (`"../capability.yaml"` for `crates/platform/<module>/`). `build()` refuses a Module naming none, so every hand-built `Policies` in a test needs one: `PolicyFile::new("capability.yaml", "id: x\nlabel: X\n")` (`crate::tests::capability()` in `app_authz`).
- **The module template has a stand-in** `crates/module-template/capability.yaml` (named `../capability.yaml`, since the template is one level shallower); `new-module.sh` skips copying it and rewrites the path to `../../capability.yaml`.
- **A registry entry needs a real route**: TanStack's `LinkProps["to"]` is the union of registered paths, so `new-module.sh` adds a route and a placeholder page with a new Capability's entry, or the SPA wouldn't type-check. It runs `biome check --write` on what it touched (if `frontend/node_modules` exists) to sort the imports it prepends.
- `make init`'s `router.test.tsx` no longer asserts "No apps yet": the first `make new-module` gives the launcher an app.
- `check-modules.sh` reads registry ids as lines matching `^ *id: "<id>",`: keep `capabilities.ts` entries one field per line (biome does).
- **`check-init` copies the tree when it starts**: format (`pnpm exec biome check --write .`, `cargo fmt`) before running it, or its `make check` fails late on lint.
- In this machine's setup the `:5432` Postgres belongs to another compose project, so `check-init`'s `docker compose exec postgres psql` fails; a `docker` shim on `PATH` mapping `compose exec -T postgres psql ..` to a local `psql -h localhost` works.
- Watch heredocs inside heredocs: a Python `<<'EOF'` script holding a bash `<<EOF` ends early. Use distinct delimiters or the Write tool.
