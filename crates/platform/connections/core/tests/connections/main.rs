// The driver names `PgPool` to receive the per-test database, like `seed`.
#![allow(clippy::disallowed_types)]

mod driver;
mod fake;

mod behaviours {
    mod access;
    mod calling;
    mod declaring;
    mod reading;
    mod secrets;
    mod updating;
}
