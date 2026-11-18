mod application;
mod db;
mod di;
mod domain;
#[cfg(feature = "seed")]
pub mod seed;
mod web;

pub use db::{MIGRATOR, SCHEMA, migrations};
pub use di::{WidgetsProvider, handlers, module, policies, provider};
pub use domain::{CreateWidget, RenameWidget, Widget, WidgetError, WidgetId};
pub use web::WidgetsEndpoints;
