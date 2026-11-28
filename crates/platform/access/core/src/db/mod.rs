//! The layer-wide `allow` below covers every file in `db/`.

#![allow(clippy::disallowed_types)]

mod assignments;
mod audit;
mod invitations;
mod migrations;
mod repository;
mod roles;
mod subjects;

pub use assignments::PgAssignmentRepository;
pub use audit::{AUDIT_ASSIGNMENTS, AUDIT_GRANTS, AUDIT_ROLES, AuditRows, PgAuditLog};
pub use invitations::{InvitationColumns, PgInvitationRepository};
pub use migrations::{MIGRATOR, SCHEMA, migrations};
pub use repository::{GrantColumns, PgGrantRepository};
pub use roles::{PgRoleRepository, RoleColumns};
pub use subjects::{PgSubjectRepository, SubjectColumns};
