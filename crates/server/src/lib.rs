//! Shared server assembly used by both the running server (`main.rs`) and the
//! DB-free `dump-spec` bin. Keeping `build_service` here guarantees the spec that
//! `dump-spec` prints is byte-for-byte the configuration the server serves.

use app_db::ModuleMigrations;
use notes::NotesApi;
use poem_openapi::OpenApiService;
use users::UsersApi;

/// Title advertised in the OpenAPI document.
pub const API_TITLE: &str = "rust-vite-template API";
/// Version advertised in the OpenAPI document.
pub const API_VERSION: &str = "0.1.0";
/// Base path the API is served under (also the client `baseUrl`).
pub const API_BASE: &str = "/api";

/// The tuple of every module's `Api`. Mind poem-openapi's 16-element tuple cap:
/// group into sub-tuples past 16 modules (spec §7).
pub type Apis = (UsersApi, NotesApi);

/// Construct the `OpenApiService` with identical config for runtime and
/// spec-dump. `server(API_BASE)` makes the generated client target `/api`.
pub fn build_service() -> OpenApiService<Apis, ()> {
    OpenApiService::new((UsersApi, NotesApi), API_TITLE, API_VERSION).server(API_BASE)
}

/// Module migrations in deterministic, dependency-aware order.
pub fn module_migrations() -> Vec<ModuleMigrations> {
    vec![users::migrations(), notes::migrations()]
}
