// Naming the pool to wire the finder is this module's DI seam (spec §4).
#![allow(clippy::disallowed_types)]

use crate::application::NoteSearchService;
use crate::db::PgNoteFinder;
use app_module::Module;
use app_web::Provide;
use sqlx::PgPool;
use std::sync::Arc;

pub struct NoteSearchProvider(PgPool);

pub fn provider(pool: PgPool) -> NoteSearchProvider {
    NoteSearchProvider(pool)
}

impl Provide for NoteSearchProvider {
    type Data = NoteSearchService;

    fn build(self) -> NoteSearchService {
        NoteSearchService::new(Arc::new(PgNoteFinder::new(self.0)))
    }
}

/// No migrations and no seed: this read module owns no data, only reads the
/// views its providers publish (spec D8).
pub fn module() -> Module {
    Module::new(provider)
}
