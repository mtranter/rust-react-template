//! The platform's `access` module (ADR 0040): it stores roles, grants and who is assigned which
//! role, applies every module's default grants once, lets admins grant, revoke, assign and
//! unassign at runtime, and keeps each server instance's rule set (and so each Principal's
//! roles) in step with them. Platform, not the app's: it stays registered after `make init`.

mod application;
mod db;
mod di;
mod domain;
mod web;

pub use application::{Modules, Operation, POLL_EVERY, Rules};
pub use db::{GrantColumns, MIGRATOR, RoleColumns, SCHEMA, migrations};
pub use di::{
    AccessProvider, apply_default_grants, assignments, module, policies, provider, rules,
};
pub use domain::{Assignment, AssignmentAction, Grant, GrantAction, GrantId, Role, RoleAction};
pub use web::AccessEndpoints;
