//! A read module (ADR 0010, 0036): lists its own copy of every root of the
//! topic it projects. It has no domain layer: there are no invariants to
//! protect, only a copy to keep and a query to run.

mod application;
mod db;
mod di;
mod web;

pub use db::migrations::migrations;
pub use di::{WidgetIndexProvider, handlers, module, provider};
pub use web::WidgetIndexEndpoints;
