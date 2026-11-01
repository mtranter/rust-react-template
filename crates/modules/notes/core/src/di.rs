// Naming the pool to wire the repository is this module's DI seam (spec §4).
#![allow(clippy::disallowed_types)]

use crate::application::NoteService;
use crate::db::{PgNoteRepository, migrations};
use app_module::Module;
use app_web::Provide;
use sqlx::PgPool;
use std::sync::Arc;
use users_api::UserDirectory;

pub struct NotesProvider(PgPool, Arc<dyn UserDirectory>);

pub fn provider(pool: PgPool, users: Arc<dyn UserDirectory>) -> NotesProvider {
    NotesProvider(pool, users)
}

impl Provide for NotesProvider {
    type Data = NoteService;

    fn build(self) -> NoteService {
        NoteService::new(Arc::new(PgNoteRepository::new(self.0)), self.1)
    }
}

/// `users` is the users module's `directory`, passed in by the server (spec D7).
pub fn module(users: fn(PgPool) -> Arc<dyn UserDirectory>) -> Module {
    let module = Module::new(move |pool: PgPool| provider(pool.clone(), users(pool)))
        .with_migrations(migrations());
    #[cfg(feature = "seed")]
    let module = module.with_seed(|pool| async move { crate::seed::standard(&pool).await });
    module
}
