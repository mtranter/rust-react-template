mod application;
mod db;
mod di;
mod domain;
mod mail;
#[cfg(feature = "seed")]
pub mod seed;
mod web;

/// The port a test driver fakes.
pub use application::{Mail, Mailer};
pub use db::{MIGRATOR, SCHEMA, migrations};
pub use di::{UsersProvider, directory, handlers, module, provider};
pub use domain::{ChangeDisplayName, Email, RegisterUser, User, UserError, UserId};
pub use web::UsersEndpoints;
