// The driver names `PgPool` to receive the per-test database, like `seed`.
#![allow(clippy::disallowed_types)]

mod driver;

mod behaviours {
    mod current_user;
    mod deleting;
    mod directory;
    mod finding;
    mod listing;
    mod registering;
    mod welcoming;
}
