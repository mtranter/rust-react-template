//! Shared by the server, `dump-spec`, `migrate`, `seed` and the composed-app
//! test, so each sees the same modules and the dumped spec is byte-for-byte the
//! one served.

mod app;
pub mod config;
mod modules;
pub mod telemetry;

pub use app::build_app;
pub use modules::{Endpoints, modules};

use app_db::ModuleMigrations;
use poem_openapi::OpenApiService;

pub const API_TITLE: &str = "rust-vite-template API";
pub const API_VERSION: &str = "0.1.0";
/// Also the generated client's `baseUrl`.
pub const API_BASE: &str = "/api";

pub fn build_service() -> OpenApiService<Endpoints, ()> {
    OpenApiService::new(Endpoints::default(), API_TITLE, API_VERSION).server(API_BASE)
}

pub fn module_migrations() -> Vec<ModuleMigrations> {
    modules().iter().filter_map(|m| m.migrations()).collect()
}
