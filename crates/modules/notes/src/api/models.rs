//! HTTP request/response shapes. Ids, timestamps, and the version are plain
//! strings so the generated TS client stays dependency-free.

use crate::application::Versioned;
use crate::domain::Note;
use poem_openapi::Object;

#[derive(Debug, Object)]
pub struct CreateNoteRequest {
    /// Author id (a plain uuid; not validated against the users module).
    pub author_id: String,
    pub title: String,
    #[oai(default)]
    pub body: String,
}

/// Replaces a note's title and body. Carries no version: the concurrency token
/// travels only in the `If-Match` header, never the body (see the handlers).
#[derive(Debug, Object)]
pub struct EditNoteRequest {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Object)]
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
