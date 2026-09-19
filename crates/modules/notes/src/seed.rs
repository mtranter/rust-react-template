//! Seed data: a deterministic, well-known dataset for tests (and, later, local
//! development). Compiled only with the `seed` feature, so it never ships in a
//! production build.
//!
//! Every seeded entity has a **named, stable id** and a fixed timestamp. Seeds
//! are built through the real domain constructors and written through the real
//! repository. See `users::seed` for the conventions.
//!
//! Id convention (spec §13): `MMMMMMMM-KKKK-0000-0000-NNNNNNNNNNNN` — module
//! number, entity kind within the module, sequence. `notes` is module `2`;
//! kind `1` is a note, kind `2` an author.
//!
//! Authors are the notes module's own ids. They are deliberately *not*
//! `users::seed` ids: modules stay isolated, seeds included.

#![allow(clippy::disallowed_types)]

use crate::application::{Expected, NoteRepository};
use crate::db::PgNoteRepository;
use crate::domain::{AuthorId, CreateNote, Note, NoteId};
use app_error::AppResult;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// The author of every seeded note — `00000002-0002-0000-0000-000000000001`.
pub const AUTHOR: AuthorId =
    AuthorId::from_uuid(Uuid::from_u128(0x00000002_0002_0000_0000_000000000001));

/// "First note" — `00000002-0001-0000-0000-000000000001`.
pub const FIRST_NOTE: NoteId =
    NoteId::from_uuid(Uuid::from_u128(0x00000002_0001_0000_0000_000000000001));
/// "Second note" — `00000002-0001-0000-0000-000000000002`.
pub const SECOND_NOTE: NoteId =
    NoteId::from_uuid(Uuid::from_u128(0x00000002_0001_0000_0000_000000000002));

/// `2025-01-01T00:00:00Z` plus `minutes`.
fn at(minutes: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_735_689_600 + minutes * 60, 0).expect("valid seed timestamp")
}

fn note(id: NoteId, title: &str, body: &str, minutes: i64) -> Note {
    Note::create(
        id,
        CreateNote {
            author_id: AUTHOR,
            title: title.into(),
            body: body.into(),
        },
        at(minutes),
    )
    .expect("seed data satisfies the domain's invariants")
}

/// "First note" by [`AUTHOR`].
pub fn first_note() -> Note {
    note(FIRST_NOTE, "First note", "hello", 0)
}

/// "Second note" by [`AUTHOR`]. Created after the first.
pub fn second_note() -> Note {
    note(SECOND_NOTE, "Second note", "world", 1)
}

/// Every note in the standard dataset, oldest first.
pub fn notes() -> Vec<Note> {
    vec![first_note(), second_note()]
}

/// Persist the standard dataset into a migrated database.
pub async fn standard(pool: &PgPool) -> AppResult<()> {
    let repo = PgNoteRepository::new(pool.clone());
    for note in notes() {
        repo.save(&note, Expected::New).await?;
    }
    Ok(())
}
