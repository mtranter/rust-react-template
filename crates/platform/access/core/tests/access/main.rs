// The driver names `PgPool` to receive the per-test database.
#![allow(clippy::disallowed_types)]

mod driver;
mod workshop;

mod behaviours {
    mod access;
    mod assigning;
    mod catalog;
    mod defaults;
    mod granting;
    mod listing;
    mod permissions;
    mod rebuilding;
    mod retiring_gates;
    mod revoking;
    mod roles;
    mod subjects;
}
