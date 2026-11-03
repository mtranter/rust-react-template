// The driver names `PgPool` to receive the per-test database, like `seed`.
#![allow(clippy::disallowed_types)]

mod driver;

mod behaviours {
    mod archiving;
    mod creating;
    mod deleting;
    mod editing;
    mod lifecycle;
    mod listing;
    mod reading;
}
