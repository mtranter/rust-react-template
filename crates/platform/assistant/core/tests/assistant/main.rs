// The driver names `PgPool` to receive the per-test database, like `seed`.
#![allow(clippy::disallowed_types)]

mod driver;

mod behaviours {
    mod deleting;
    mod listing;
    mod reading;
    mod scoping;
    mod starting;
    mod status;
}
