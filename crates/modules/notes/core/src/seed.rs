//! Builds the notes `notes_api::seed` describes, adding what isn't published.

#![allow(clippy::disallowed_types)]

use crate::application::{Expected, NoteRepository, Transactions};
use crate::db::PgNoteRepository;
use crate::domain::{AuthorId, CreateNote, Note, NoteId};
use app_db::PgTransactions;
use app_error::AppResult;
use notes_api::{seed, v1};
use sqlx::PgPool;

pub const ALICES_NOTE: NoteId = NoteId::from_uuid(seed::ALICES_NOTE);
pub const BOBS_NOTE: NoteId = NoteId::from_uuid(seed::BOBS_NOTE);
pub const ORPHANED_NOTE: NoteId = NoteId::from_uuid(seed::ORPHANED_NOTE);

fn note(published: v1::Note, body: &str) -> Note {
    Note::create(
        NoteId::from_uuid(published.id),
        CreateNote {
            author_id: AuthorId::from_uuid(published.author_id),
            title: published.title,
            body: body.into(),
        },
        published.created_at,
    )
    .expect("seed data satisfies the domain's invariants")
}

pub fn alices_note() -> Note {
    note(seed::alices_note(), "milk, eggs")
}

pub fn bobs_note() -> Note {
    note(seed::bobs_note(), "new chain")
}

pub fn orphaned_note() -> Note {
    note(seed::orphaned_note(), "")
}

pub fn notes() -> Vec<Note> {
    vec![alices_note(), bobs_note(), orphaned_note()]
}

/// Saves only what's missing, so a deployed environment can re-run it on every
/// release without failing or undoing edits to seeded entities.
pub async fn standard(pool: &PgPool) -> AppResult<()> {
    let repo = PgNoteRepository::new(pool.clone());
    let mut tx = PgTransactions::new(pool.clone()).begin().await?;
    for note in notes() {
        if repo.get(note.id()).await?.is_none() {
            repo.save(&mut tx, &note, Expected::New).await?;
        }
    }
    tx.commit().await
}
