//! The platform's `access` module (ADR 0040): it stores grants, applies every module's default
//! grants once, lets admins grant and revoke at runtime, and keeps each server instance's rule
//! set in step with them. Platform, not the app's: it stays registered after `make init`.

mod application;
mod db;
mod di;
mod domain;
mod web;

pub use application::{Modules, Operation, POLL_EVERY, Rules};
pub use db::{GrantColumns, MIGRATOR, SCHEMA, migrations};
pub use di::{AccessProvider, apply_default_grants, module, policies, provider, rules};
pub use domain::{Grant, GrantAction, GrantId};
pub use web::AccessEndpoints;
