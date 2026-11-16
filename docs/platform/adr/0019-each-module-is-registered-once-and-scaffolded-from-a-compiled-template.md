# Each Module is registered once, and scaffolded from a template that compiles

Adding a Module took about nine hand edits across shared files, and several omissions failed silently: the code compiled, every Module's own tests passed, and production broke. Now a Module is registered in one place, `crates/server/src/modules.rs`: its `Endpoints` in the `Endpoints` tuple (the Spec's type and, via `Default`, its value) and its `module(..)` in `modules()`. A `Module` bundles the provider, its Worker handlers, its migrations and, under the `seed` feature, its Seed; the Server derives everything else from that one ordered list: the migrations it runs, the request data `build_app()` adds, the handlers the Worker runs and what the `seed` bin runs. A Module using another's Module API takes the provider's constructor as an argument, so it is listed after it.

`make new-module` scaffolds a write Module from `crates/module-template/{core,api}`: every Layer, a versioned Aggregate with create, read, rename and list publishing its Events, a migration, a Seed and tests that pass as generated. It allocates the Module number, applies the migrations, refreshes `.sqlx` and registers the Module everywhere. The template is itself two workspace crates (`widgets`, `widgets-api`), compiled, linted, tested and held to `check-deps` and `check-sql` like any Module, but registered with no Server, so it can't rot. CI also scaffolds one to test the generator. A Read Module is rare enough to copy `note-search` by hand.

## Considered options

- **A template of text files with placeholders.** Nothing would compile or test it until someone generated from it.

## Consequences

- Each Module's own tests wire it themselves, so two checks cover the Server. `composed_app.rs` migrates and seeds through the registry, builds the real app and requests every `GET` in the Spec, failing on any 5xx (a missing provider or migration). `make check-modules` fails on a Module core missing from `modules.rs`, the Server's dependencies or `seed` feature, or either `deny.toml` list, and on a missing or duplicated Module number.
- The template's migrations reach the dev database only for `make prepare`, so `.sqlx` covers its queries.
