// The driver names `PgPool` to receive the per-test database, like `seed`.
#![allow(clippy::disallowed_types)]

mod driver;

mod behaviours {
    mod access;
    mod archiving;
    mod archiving_at;
    mod creating;
    mod deleting;
    mod editing;
    mod items;
    mod lifecycle;
    mod listing;
    mod reading;
    mod retrying;
    mod scoping;
}
