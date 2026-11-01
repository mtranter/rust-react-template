// Naming the pool to wire the repository is this module's DI seam (spec §4).
#![allow(clippy::disallowed_types)]

use crate::application::NoteService;
use crate::db::{PgNoteRepository, migrations};
use app_module::{Context, Module};
use app_persistence::Handlers;
use app_web::Provide;
use std::sync::Arc;
use users_api::UserDirectory;
use users_api::events::v1::UserDeleted;

pub struct NotesProvider(NoteService);

pub fn provider(ctx: &Context, users: Arc<dyn UserDirectory>) -> NotesProvider {
    NotesProvider(service(ctx, users))
}

impl Provide for NotesProvider {
    type Data = NoteService;

    fn build(self) -> NoteService {
        self.0
    }
}

fn service(ctx: &Context, users: Arc<dyn UserDirectory>) -> NoteService {
    NoteService::new(
        Arc::new(PgNoteRepository::new(ctx.pool())),
        users,
        ctx.publisher(),
        ctx.transactions(),
        ctx.clock(),
    )
}

/// What the worker runs for notes: its reactions to other modules' events.
pub fn handlers(ctx: &Context, users: Arc<dyn UserDirectory>) -> Handlers {
    Handlers::new().on::<UserDeleted>("notes.archive_deleted_authors_notes", service(ctx, users))
}

/// `users` is the users module's `directory`, passed in by the server (spec D7).
pub fn module(users: fn(&Context) -> Arc<dyn UserDirectory>) -> Module {
    let module = Module::new(move |ctx: &Context| provider(ctx, users(ctx)))
        .with_handlers(move |ctx| handlers(ctx, users(ctx)))
        .with_migrations(migrations());
    #[cfg(feature = "seed")]
    let module = module.with_seed(|pool| async move { crate::seed::standard(&pool).await });
    module
}
