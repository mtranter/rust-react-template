//! Behaviour of the users module, tested outside-in: every test drives the real
//! HTTP surface through api → application → domain → db, in a fresh
//! `#[sqlx::test]` database seeded with the well-known dataset (spec §13).
//!
//! Tests speak in domain terms via [`driver::Users`]; only the driver knows about
//! routes, JSON, status codes, and tokens.

// The driver names `PgPool` to receive the per-test database, like `seed`.
#![allow(clippy::disallowed_types)]

mod driver;

mod current_user;
mod finding;
mod registering;
