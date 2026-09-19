//! `users` module — a self-contained, liftable unit of the domain.
//!
//! Four layers as sub-module folders, each re-exported from its `mod.rs`:
//!
//! - [`api`] — HTTP surface. Translates requests into commands.
//! - [`application`] — services that coordinate a use case: load an aggregate
//!   from a repository, run a domain action, save it. Defines the `Repository`.
//! - [`domain`] — aggregates, value objects, commands, domain errors. Pure.
//! - [`db`] — the sqlx implementation of the application's `Repository`.
//!
//! Dependencies point inward: `api → application → domain`, and `db →
//! application`. Only the items re-exported below are visible to the outside,
//! plus [`seed`] (the well-known test dataset) under the `seed` feature.

mod api;
mod application;
mod db;
mod di;
mod domain;
#[cfg(feature = "seed")]
pub mod seed;

pub use api::UsersApi;
pub use db::{MIGRATOR, SCHEMA, migrations};
pub use di::{UsersProvider, provider};
pub use domain::{ChangeDisplayName, Email, RegisterUser, User, UserError, UserId};
