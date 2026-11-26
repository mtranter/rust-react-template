// Naming the pool to wire the repository is this module's DI seam (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::{ArchiveNote, NoteService, PurgeArchivedNotes, ShareNote, noticeboard};
use crate::db::{PgNoteRepository, migrations};
use crate::domain::Note;
use crate::web::OPERATIONS;
use app_authz::{Policies, PoliciesBuilder, policy_file};
use app_module::{Context, Module};
use app_persistence::Handlers;
use app_web::Provide;
use connections_api::Clients;
use notes_api::events::v1::NoteTopic;
use std::sync::Arc;
use users_api::UserDirectory;
use users_api::events::v1::UserDeleted;

pub struct NotesProvider(NoteService);

pub fn provider(
    ctx: &Context,
    users: Arc<dyn UserDirectory>,
    connections: Arc<dyn Clients>,
) -> NotesProvider {
    NotesProvider(service(ctx, users, connections))
}

impl Provide for NotesProvider {
    type Data = NoteService;

    fn build(self) -> NoteService {
        self.0
    }
}

fn service(
    ctx: &Context,
    users: Arc<dyn UserDirectory>,
    connections: Arc<dyn Clients>,
) -> NoteService {
    NoteService::new(
        Arc::new(PgNoteRepository::new(ctx.pool())),
        users,
        ctx.publisher(),
        ctx.deferred_jobs(),
        ctx.transactions(),
        ctx.clock(),
        ctx.authorizer(),
        connections,
    )
}

/// The notes module's `note` resource and the `author` scope, and
/// what `user` and `admin` are granted by default (ADR 0040).
pub fn policies() -> PoliciesBuilder {
    Policies::builder("notes")
        .capability(policy_file!("../../capability.yaml"))
        .resource::<Note>(policy_file!("policies/note.yaml"))
        .derived_roles(policy_file!("policies/derived_roles.yaml"))
        .default_grants(policy_file!("policies/default_grants.yaml"))
        .labels(policy_file!("policies/labels.yaml"))
}

/// What the worker runs for notes: its reactions to other modules' events,
/// the archives its notes defer to their `archiveAt`, the pins to the
/// noticeboard `share_note` defers, and the nightly purge of notes archived
/// long ago.
pub fn handlers(
    ctx: &Context,
    users: Arc<dyn UserDirectory>,
    connections: Arc<dyn Clients>,
) -> Handlers {
    let service = service(ctx, users, connections);
    Handlers::new()
        .on::<UserDeleted>("notes.archive_deleted_authors_notes", service.clone())
        .runs::<ArchiveNote>(service.clone())
        .runs::<ShareNote>(service.clone())
        .every::<PurgeArchivedNotes>(service)
}

/// `users` is the users module's `directory`, and `connections` what it calls the
/// noticeboard through, `connections::clients`: both passed in by the server
/// (ADR 0008, 0050). It declares the `noticeboard` Connection, and under `seed`
/// the values `make dev` gives it, pointing it at `fake-noticeboard`.
pub fn module(
    users: fn(&Context) -> Arc<dyn UserDirectory>,
    connections: fn(&Context) -> Arc<dyn Clients>,
) -> Module {
    let module = Module::new(move |ctx: &Context| provider(ctx, users(ctx), connections(ctx)))
        .with_policies(policies)
        .performing(OPERATIONS)
        .publishes::<NoteTopic>()
        .declaring(noticeboard())
        .with_handlers(move |ctx| handlers(ctx, users(ctx), connections(ctx)))
        .with_migrations(migrations());
    #[cfg(feature = "seed")]
    let module = module
        .declaring(crate::seed::dev_noticeboard())
        .with_seed(|ctx| async move { crate::seed::standard(&ctx.pool()).await });
    module
}
