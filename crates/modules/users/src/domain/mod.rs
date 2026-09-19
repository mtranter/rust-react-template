//! Domain layer: aggregates, value objects, commands, and domain errors. Depends
//! only on pure crates — no poem, no sqlx, no repositories, no async. Operations
//! that can fail return `Result<_, UserError>`; the application layer decides
//! what those failures mean to a caller.
//!
//! - `ids` — strongly-typed identifiers.
//! - `email` — the `Email` value object (validated on construction).
//! - `user` — the `User` aggregate and its domain actions.
//! - `commands` — the intents a domain action accepts.
//! - `error` — the module's domain error enum.

mod commands;
mod email;
mod error;
mod ids;
mod user;

pub use commands::{ChangeDisplayName, RegisterUser};
pub use email::Email;
pub use error::UserError;
pub use ids::UserId;
pub use user::User;
