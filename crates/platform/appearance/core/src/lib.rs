mod application;
mod db;
mod di;
mod domain;
mod web;

pub use db::{MIGRATOR, SCHEMA, migrations};
pub use di::{AppearanceProvider, module, policies, provider};
pub use domain::{Theme, ThemeAction};
pub use web::AppearanceEndpoints;
