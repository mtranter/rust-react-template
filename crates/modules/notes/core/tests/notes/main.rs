// The driver names `PgPool` to receive the per-test database, like `seed`.
#![allow(clippy::disallowed_types)]

mod driver;

mod behaviours {
    mod archiving;
    mod creating;
    mod editing;
    mod listing;
    mod reading;
}
