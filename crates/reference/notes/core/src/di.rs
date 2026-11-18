// Naming the pool to wire the repository is this module's DI seam (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::NoteService;
use crate::db::{PgNoteRepository, migrations};
use crate::domain::Note;
use crate::web::OPERATIONS;
use app_authz::{Policies, PoliciesBuilder, policy_file};
use app_module::{Context, Module};
use app_persistence::Handlers;
use app_web::Provide;
use notes_api::events::v1::NoteTopic;
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
        ctx.authorizer(),
    )
}

/// The notes module's gate, its `note` resource and the `author` scope, and
/// what `user` and `admin` are granted by default (ADR 0040).
pub fn policies() -> PoliciesBuilder {
    Policies::builder("notes")
        .resource::<Note>(policy_file!("policies/note.yaml"))
        .derived_roles(policy_file!("policies/derived_roles.yaml"))
        .default_grants(policy_file!("policies/default_grants.yaml"))
}

/// What the worker runs for notes: its reactions to other modules' events.
pub fn handlers(ctx: &Context, users: Arc<dyn UserDirectory>) -> Handlers {
    Handlers::new().on::<UserDeleted>("notes.archive_deleted_authors_notes", service(ctx, users))
}

/// `users` is the users module's `directory`, passed in by the server (ADR 0008).
pub fn module(users: fn(&Context) -> Arc<dyn UserDirectory>) -> Module {
    let module = Module::new(move |ctx: &Context| provider(ctx, users(ctx)))
        .with_policies(policies)
        .performing(OPERATIONS)
        .publishes::<NoteTopic>()
        .with_handlers(move |ctx| handlers(ctx, users(ctx)))
        .with_migrations(migrations());
    #[cfg(feature = "seed")]
    let module = module.with_seed(|ctx| async move { crate::seed::standard(&ctx.pool()).await });
    module
}
