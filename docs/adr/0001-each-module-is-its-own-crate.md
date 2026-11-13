# Each Module is its own crate, in a modular monolith

The template is a Modular Monolith: many Modules, compiled and deployed as one Server. Each Module is its own crate (`crates/modules/<module>/core`, package `<module>`), and its four Layers are folders inside that crate. A crate can import only what its `Cargo.toml` lists, so "a Module never reaches into another" stops being a convention and becomes a compile error. A Module that declares its own dependencies and owns its own schema (ADR 0021) can also be lifted out into a service of its own with little surgery.

## Considered options

- **A crate per Layer per Module.** The most enforcement, with three times the crates and Cargo churn. Over-engineered for a template.
- **One crate with `mod` boundaries.** Isolation would be a convention only, which nothing checks.
- **`rust_arkitect`** for layering assertions inside a crate. Largely unnecessary once each Module is a crate; not wired. Worth revisiting only if a Module splits its Layers into crates.

## Consequences

- More crates to wire. Registering one is a single entry plus generated edits (ADR 0019).
- poem-openapi's `OpenApi` tuple holds at most 16 elements, and std derives `Default` only up to 12, so past 12 Modules the `Endpoints` tuple nests sub-tuples.
- The Cargo graph is the first of several isolation checks (ADR 0017); it holds only while nobody adds the dependency line, which `make check-deps` guards.
