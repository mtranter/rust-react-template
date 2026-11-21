// The driver names `PgPool` to receive the per-test database, like `seed`.
#![allow(clippy::disallowed_types)]

mod driver;

mod behaviours {
    mod deciding;
    mod deleting;
    mod ending;
    mod listing;
    mod proposing;
    mod reading;
    mod scoping;
    mod sending;
    mod starting;
    mod status;
}
