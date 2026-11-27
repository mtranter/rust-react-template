mod application;
mod db;
mod di;
mod domain;
mod web;

pub use db::{MIGRATOR, SCHEMA, migrations};
pub use di::{Appearance, AppearanceProvider, module, policies, provider};
pub use domain::{AppVisibility, AppVisibilityAction, Theme, ThemeAction};
pub use web::AppearanceEndpoints;
