//! The layer-wide `allow` below covers every file in `db/`.

#![allow(clippy::disallowed_types)]

mod assignments;
mod migrations;
mod repository;
mod roles;

pub use assignments::PgAssignmentRepository;
pub use migrations::{MIGRATOR, SCHEMA, migrations};
pub use repository::{GrantColumns, PgGrantRepository};
pub use roles::{PgRoleRepository, RoleColumns};
