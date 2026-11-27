// The driver names `PgPool` to receive the per-test database, like `seed`.
#![allow(clippy::disallowed_types)]

mod driver;

mod behaviours {
    mod access;
    mod apps;
    mod reading;
    mod updating;
}
