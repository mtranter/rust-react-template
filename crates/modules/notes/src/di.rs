//! Composition root: build the [`NoteService`] over its Postgres repository.
//!
//! This is the one place in the module's public surface that names the pool (the
//! api layer no longer does). The server hands the pool over via [`provider`] and
//! attaches the built service with `app_web::Accept::accept`; handlers then
//! receive it as `Data<&NoteService>`.

// Naming the pool to wire the repository is this module's DI seam (spec §4).
#![allow(clippy::disallowed_types)]

use crate::application::NoteService;
use crate::db::PgNoteRepository;
use app_web::Provide;
use sqlx::PgPool;
use std::sync::Arc;

/// The notes module's request-data contribution: a [`NoteService`] over `pool`.
pub struct NotesProvider(PgPool);

/// Build the notes [`Provide`]r from a connection pool.
pub fn provider(pool: PgPool) -> NotesProvider {
    NotesProvider(pool)
}

impl Provide for NotesProvider {
    type Data = NoteService;

    fn build(self) -> NoteService {
        NoteService::new(Arc::new(PgNoteRepository::new(self.0)))
    }
}
