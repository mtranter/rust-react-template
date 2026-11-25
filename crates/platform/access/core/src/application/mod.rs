//! `Version`, `Versioned`, `Expected`, `Tx` and the listing types are re-exported from
//! `app-persistence` so they read as this layer's vocabulary while staying identical across modules.

mod assignments;
mod catalog;
mod directory;
mod error;
mod events;
mod gates;
mod invitations;
mod repository;
mod roles;
mod rules;
mod service;
mod subjects;

pub use app_persistence::{
    EventPublisher, ListQuery, Listed, PendingEvent, Transactions, Tx, Version, Versioned,
};
pub use catalog::{CatalogAction, CatalogCapability, CatalogResource};
pub use directory::{Created, DirectoryStatus, DirectoryUser, IdentityDirectory};
pub(crate) use events::scope_name;
pub use invitations::InvitationListing;
pub use repository::{
    AssignmentRepository, GrantFilter, GrantRepository, GrantSortKey, InvitationFilter,
    InvitationRepository, InvitationSortKey, RoleRepository, RoleSortKey, SubjectFilter,
    SubjectRepository, SubjectSortKey,
};
pub use rules::{POLL_EVERY, Rules};
pub use service::{
    AccessService, GrantInBulk, GrantedInBulk, Listing, Modules, Operation, Permission,
};
pub use subjects::{HeldRole, SignIns, SubjectListing};
