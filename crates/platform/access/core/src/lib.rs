//! The platform's `access` module (ADR 0040): it stores roles, grants and who is assigned which
//! role, records who signs in, applies every module's default grants once, lets admins grant, revoke, assign and
//! unassign at runtime, and keeps each server instance's rule set (and so each Principal's
//! roles) in step with them. Platform, not the app's: it stays registered after `make init`.

mod application;
mod db;
mod di;
mod domain;
mod web;

pub use application::{Modules, Operation, POLL_EVERY, Rules};
pub use db::{GrantColumns, MIGRATOR, RoleColumns, SCHEMA, SubjectColumns, migrations};
pub use di::{
    AccessProvider, apply_default_grants, assignments, module, policies, provider, rules, sign_ins,
};
pub use domain::{
    Assignment, AssignmentAction, Grant, GrantAction, GrantId, Role, RoleAction, Subject,
    SubjectAction,
};
pub use web::AccessEndpoints;
