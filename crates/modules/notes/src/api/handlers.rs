//! The notes `#[OpenApi]` operations.

use super::models::{CreateNoteRequest, EditNoteRequest, NoteResponse};
use crate::application::{NoteService, Versioned};
use crate::domain::{AuthorId, CreateNote, EditNote, Note, NoteId};
use app_error::AppError;
use app_web::{ApiResultExt, Page, Pagination, WithETag, if_match_version};
use poem::web::Data;
use poem_openapi::{
    OpenApi, Tags,
    param::{Header, Path, Query},
    payload::Json,
};

/// Every single-item note read/write returns the note with its version as an
/// `ETag` header (and in the body) — the token a client's next edit sends back.
type NoteResult = WithETag<NoteResponse>;

#[derive(Tags)]
enum ApiTag {
    Notes,
}

fn note_id(raw: &str) -> poem::Result<NoteId> {
    raw.parse()
        .map_err(|_| AppError::validation("invalid note id"))
        .api()
}

/// A saved/loaded note as its ETag-carrying response (version in header and body).
fn tagged(note: Versioned<Note>) -> NoteResult {
    let version = note.version;
    NoteResult::new(note.into(), version)
}

/// The notes module API. Unit struct — no state. The service is injected as
/// request data (`Data<&NoteService>`), wired by the module's `provider`.
pub struct NotesApi;

#[OpenApi(prefix_path = "/notes", tag = "ApiTag::Notes")]
impl NotesApi {
    /// Create a note.
    #[oai(path = "/", method = "post")]
    async fn create(
        &self,
        notes: Data<&NoteService>,
        body: Json<CreateNoteRequest>,
    ) -> poem::Result<NoteResult> {
        let author_id: AuthorId = body
            .0
            .author_id
            .parse()
            .map_err(|_| AppError::validation("invalid author_id"))
            .api()?;
        let note = notes
            .0
            .create(CreateNote {
                author_id,
                title: body.0.title,
                body: body.0.body,
            })
            .await
            .api()?;
        Ok(tagged(note))
    }

    /// Fetch a note by id. The response carries its version in the `ETag` header
    /// (and body) — the token a subsequent edit must send back in `If-Match`.
    #[oai(path = "/:id", method = "get")]
    async fn get(&self, notes: Data<&NoteService>, id: Path<String>) -> poem::Result<NoteResult> {
        let note = notes.0.get(note_id(&id.0)?).await.api()?;
        Ok(tagged(note))
    }

    /// Edit a note's title and body. Requires `If-Match` with the version the
    /// client last read; a stale value is refused with `412` and changes nothing.
    #[oai(path = "/:id", method = "patch")]
    async fn edit(
        &self,
        notes: Data<&NoteService>,
        id: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
        body: Json<EditNoteRequest>,
    ) -> poem::Result<NoteResult> {
        let expected = if_match_version(if_match.0)?;
        let note = notes
            .0
            .edit(
                EditNote {
                    note_id: note_id(&id.0)?,
                    title: body.0.title,
                    body: body.0.body,
                },
                expected,
            )
            .await
            .api()?;
        Ok(tagged(note))
    }

    /// List notes, most-recent first (paginated).
    #[oai(path = "/", method = "get")]
    async fn list(
        &self,
        notes: Data<&NoteService>,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
    ) -> poem::Result<Json<Page<NoteResponse>>> {
        let page = Pagination::new(limit.0, offset.0);
        let (list, total) = notes.0.list(page.limit, page.offset).await.api()?;
        let items = list.into_iter().map(NoteResponse::from).collect();
        Ok(Json(Page::new(items, total, page)))
    }
}
