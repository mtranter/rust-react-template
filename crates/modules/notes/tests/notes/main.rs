//! Behaviour of the notes module, tested outside-in: every test drives the real
//! HTTP surface through api → application → domain → db, in a fresh
//! `#[sqlx::test]` database seeded with the well-known dataset (spec §13).
//!
//! Tests speak in domain terms via [`driver::Notes`]; only the driver knows about
//! routes, JSON, and status codes.

// The driver names `PgPool` to receive the per-test database, like `seed`.
#![allow(clippy::disallowed_types)]

mod driver;

mod creating;
mod editing;
mod reading;
