//! Ids, timestamps, and the version are plain strings so the generated TS client
//! stays dependency-free.

use crate::application::{NoteSortKey, Versioned};
use crate::domain::Note;
use app_persistence::{Direction, Sort};
use app_web::SortParam;
use poem_openapi::{Enum, Object};

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct CreateNoteRequest {
    /// Author id: the uuid of a registered user.
    pub author_id: String,
    pub title: String,
    #[oai(default)]
    pub body: String,
}

/// Replaces a note's title and body. Carries no version: the concurrency token
/// travels only in the `If-Match` header, never the body (see the handlers).
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct EditNoteRequest {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct NoteResponse {
    pub id: String,
    pub author_id: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
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
