//! API layer for notes. Unit struct; pool via `Data<&PgPool>`. Handlers turn
//! requests into commands and call the application service.
//!
//! - `handlers` — the `#[OpenApi]` operations.
//! - `models` — request/response shapes and their conversions from domain types.

mod handlers;
mod models;

pub use handlers::NotesApi;
