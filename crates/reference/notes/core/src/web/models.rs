//! Ids, timestamps, and the version are plain strings so the generated TS client
//! stays dependency-free.

use crate::application::{NoteSortKey, Versioned};
use crate::domain::Note;
use app_persistence::{Direction, Sort};
use app_web::SortParam;
use poem_openapi::{Enum, Object};

/// Its author is the caller.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct CreateNoteRequest {
    pub title: String,
    #[oai(default)]
    pub body: String,
    /// When to archive the note (RFC 3339), if ever: from then on it is
    /// read-only. A time already past archives it at once.
    pub archive_at: Option<String>,
}

/// Replaces a note's title, body and `archiveAt`: one left out is cleared,
/// cancelling the archive. Carries no version: the concurrency token travels
/// only in the `If-Match` header, never the body (see the handlers).
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct EditNoteRequest {
    pub title: String,
    pub body: String,
    /// When to archive the note (RFC 3339), if ever; a new time reschedules
    /// it, and a time already past archives it at once. Send back the one read
    /// to keep it.
    pub archive_at: Option<String>,
}

/// Adds a line to a note's checklist. Like an edit, it carries no version.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct AddItemRequest {
    pub text: String,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct ItemResponse {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct NoteResponse {
    pub id: String,
    pub author_id: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
    /// When the note is due to archive itself, as its author chose.
    #[oai(skip_serializing_if_is_none)]
    pub archive_at: Option<String>,
    /// When the note was archived: at its `archiveAt`, or because its author
    /// was deleted. An archived note can't be edited or deleted.
    #[oai(skip_serializing_if_is_none)]
    pub archived_at: Option<String>,
    /// Its checklist, in the order the items were added. An item is part of
    /// the note: changing one changes the note's version.
    pub items: Vec<ItemResponse>,
    /// Optimistic-concurrency version. Echo it back in `If-Match` to edit this
    /// note; a stale value is refused with `412 Precondition Failed`. Also
    /// mirrored in the `ETag` response header for single-item reads.
    pub version: String,
}

impl From<Versioned<Note>> for NoteResponse {
    fn from(versioned: Versioned<Note>) -> Self {
        let Versioned { entity: n, version } = versioned;
        Self {
            id: n.id().to_string(),
            author_id: n.author_id().to_string(),
            title: n.title().to_string(),
            body: n.body().to_string(),
            created_at: n.created_at().to_rfc3339(),
            archive_at: n.archive_at().map(|at| at.to_rfc3339()),
            archived_at: n.archived_at().map(|at| at.to_rfc3339()),
            items: n
                .items()
                .iter()
                .map(|item| ItemResponse {
                    id: item.id().to_string(),
                    text: item.text().to_string(),
                })
                .collect(),
            version: version.as_i64().to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Enum)]
pub enum NoteSort {
    #[oai(rename = "title")]
    TitleAsc,
    #[oai(rename = "-title")]
    TitleDesc,
    #[oai(rename = "createdAt")]
    CreatedAtAsc,
    #[oai(rename = "-createdAt")]
    CreatedAtDesc,
}

impl SortParam for NoteSort {
    type Key = NoteSortKey;

    fn sort(self) -> Sort<NoteSortKey> {
        let (key, direction) = match self {
            Self::TitleAsc => (NoteSortKey::Title, Direction::Asc),
            Self::TitleDesc => (NoteSortKey::Title, Direction::Desc),
            Self::CreatedAtAsc => (NoteSortKey::CreatedAt, Direction::Asc),
            Self::CreatedAtDesc => (NoteSortKey::CreatedAt, Direction::Desc),
        };
        Sort { key, direction }
    }
}
