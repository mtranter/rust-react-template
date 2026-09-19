//! Composition root: build the [`UserService`] over its Postgres repository.
//!
//! This is the one place in the module's public surface that names the pool (the
//! api layer no longer does). The server hands the pool over via [`provider`] and
//! attaches the built service with `app_web::Accept::accept`; handlers then
//! receive it as `Data<&UserService>`.

// Naming the pool to wire the repository is this module's DI seam (spec §4).
#![allow(clippy::disallowed_types)]

use crate::application::UserService;
use crate::db::PgUserRepository;
use app_web::Provide;
use sqlx::PgPool;
use std::sync::Arc;

/// The users module's request-data contribution: a [`UserService`] over `pool`.
pub struct UsersProvider(PgPool);

/// Build the users [`Provide`]r from a connection pool.
pub fn provider(pool: PgPool) -> UsersProvider {
    UsersProvider(pool)
}

impl Provide for UsersProvider {
    type Data = UserService;

    fn build(self) -> UserService {
        UserService::new(Arc::new(PgUserRepository::new(self.0)))
    }
}
