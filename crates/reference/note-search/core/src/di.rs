// Naming the pool to wire the finder is this module's DI seam (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::{NoteSearchService, NoteSummary};
use crate::db::{AuthorRows, COPY_AUTHORS, COPY_NOTES, NoteRows, PgNoteFinder, migrations};
use crate::web::OPERATIONS;
use app_authz::{Authorizer, Policies, PoliciesBuilder, policy_file};
use app_db::Projecting;
use app_module::{Context, Module};
use app_persistence::Handlers;
use app_web::Provide;
use notes_api::events::v1::NoteTopic;
use sqlx::PgPool;
use std::sync::Arc;
use users_api::events::v1::UserTopic;

pub struct NoteSearchProvider(PgPool, Arc<Authorizer>);

pub fn provider(ctx: &Context) -> NoteSearchProvider {
    NoteSearchProvider(ctx.pool(), ctx.authorizer())
}

impl Provide for NoteSearchProvider {
    type Data = NoteSearchService;

    fn build(self) -> NoteSearchService {
        NoteSearchService::new(Arc::new(PgNoteFinder::new(self.0)), self.1)
    }
}

/// What the worker runs for note-search: the projections keeping its copy of
/// every note and author, from the earliest each topic holds.
pub fn handlers(_ctx: &Context) -> Handlers {
    Handlers::new()
        .project::<NoteTopic>(COPY_NOTES, NoteRows)
        .project::<UserTopic>(COPY_AUTHORS, AuthorRows)
}

/// The note-search module's gate and its `noteSummary` resource, and what
/// `user` and `admin` are granted by default (ADR 0040).
pub fn policies() -> PoliciesBuilder {
    Policies::builder("note-search")
        .resource::<NoteSummary>(policy_file!("policies/note_summary.yaml"))
        .default_grants(policy_file!("policies/default_grants.yaml"))
        .labels(policy_file!("policies/labels.yaml"))
}

/// No seed: everything it holds is a copy, filled from the providers' events
/// (ADR 0036).
pub fn module() -> Module {
    Module::new(provider)
        .with_policies(policies)
        .performing(OPERATIONS)
        .with_handlers(handlers)
        .with_migrations(migrations::migrations())
}
