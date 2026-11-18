// The driver names `PgPool` to receive the per-test database.
#![allow(clippy::disallowed_types)]

mod driver;
mod workshop;

mod behaviours {
    mod access;
    mod catalog;
    mod defaults;
    mod granting;
    mod listing;
    mod rebuilding;
    mod revoking;
    mod roles;
}
