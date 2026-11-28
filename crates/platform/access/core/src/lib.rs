//! The platform's `access` module (ADR 0040): it stores roles, grants and who is assigned which
//! role, invites people through the identity provider, records who signs in, applies every module's default grants once, lets admins grant, revoke, assign and
//! unassign at runtime, and keeps each server instance's rule set (and so each Principal's
//! roles) in step with them. Platform, not the app's: it stays registered after `make init`.

mod application;
mod cognito;
mod db;
mod di;
mod domain;
mod web;

pub use application::{
    AuditChange, AuditEntry, AuditEntryAction, Created, DirectoryStatus, DirectoryUser,
    IdentityDirectory, Modules, Operation, POLL_EVERY, Rules,
};
pub use cognito::CognitoDirectory;
pub use db::{
    GrantColumns, InvitationColumns, MIGRATOR, RoleColumns, SCHEMA, SubjectColumns, migrations,
};
pub use di::{
    AccessProvider, apply_default_grants, assignments, directory, handlers, module, policies,
    provider, rules, sign_ins,
};
pub use domain::{
    Assignment, AssignmentAction, Email, Grant, GrantAction, GrantId, Invitation, InvitationAction,
    Role, RoleAction, Subject, SubjectAction,
};
pub use web::AccessEndpoints;
