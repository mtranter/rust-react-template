//! Builds the notes `notes_api::seed` describes, adding what isn't published.

#![allow(clippy::disallowed_types)]

use crate::application::{Expected, NoteRepository, Transactions};
use crate::db::PgNoteRepository;
use crate::domain::{AuthorId, CreateNote, ItemId, Note, NoteId};
use app_db::PgTransactions;
use app_domain::well_known_id;
use app_error::AppResult;
use notes_api::{seed, v1};
use sqlx::PgPool;

pub const ALICE_NOTE_ID_SHOPPING_LIST: NoteId =
    NoteId::from_uuid(seed::ALICE_NOTE_ID_SHOPPING_LIST);
pub const BOB_NOTE_ID_BIKE_REPAIRS: NoteId = NoteId::from_uuid(seed::BOB_NOTE_ID_BIKE_REPAIRS);
pub const NOTE_ID_ORPHANED: NoteId = NoteId::from_uuid(seed::NOTE_ID_ORPHANED);
/// Items aren't published, so their ids live here; kind `3` is an item.
pub const SHOPPING_LIST_ITEM_ID_MILK: ItemId = ItemId::from_uuid(well_known_id(seed::MODULE, 3, 1));
pub const SHOPPING_LIST_ITEM_ID_EGGS: ItemId = ItemId::from_uuid(well_known_id(seed::MODULE, 3, 2));

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
    .0
}

pub fn alices_note() -> Note {
    let mut note = note(seed::alices_note(), "for the weekend");
    for (id, text) in [
        (SHOPPING_LIST_ITEM_ID_MILK, "Milk"),
        (SHOPPING_LIST_ITEM_ID_EGGS, "Eggs"),
    ] {
        let _ = note
            .add_item(id, text)
            .expect("seed data satisfies the domain's invariants");
    }
    note
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
