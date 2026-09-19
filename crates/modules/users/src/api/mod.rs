//! API layer: the module's HTTP surface. A `#[OpenApi]` impl on a **unit struct**
//! — the pool is injected per-request via `Data<&PgPool>`, not held as state, so
//! `dump-spec` needs no database. Handlers turn requests into domain commands,
//! call the application service, and translate results to HTTP via `app-web`'s
//! `.api()` bridge. No business logic lives here.
//!
//! - `handlers` — the `#[OpenApi]` operations.
//! - `models` — request/response shapes and their conversions from domain types.

mod handlers;
mod models;

pub use handlers::UsersApi;
