use super::models::{AddItemRequest, CreateNoteRequest, EditNoteRequest, NoteResponse, NoteSort};
use crate::application::{NoteService, Versioned};
use crate::domain::{AuthorId, CreateNote, EditNote, ItemId, Note, NoteId};
use app_error::AppError;
use app_web::{ApiResult, ListResult, WithETag, if_match_version, paged, sorts};
use poem::web::Data;
use poem_openapi::{
    ApiResponse, OpenApi, Tags,
    param::{Header, Path, Query},
    payload::Json,
};

type NoteResult = WithETag<NoteResponse>;

#[derive(Tags)]
enum ApiTag {
    Notes,
}

#[derive(ApiResponse)]
enum Deleted {
    #[oai(status = 204)]
    Deleted,
}

fn note_id(raw: &str) -> ApiResult<NoteId> {
    raw.parse()
        .map_err(|_| AppError::validation("invalid note id").into())
}

fn parse_item_id(raw: &str) -> ApiResult<ItemId> {
    raw.parse()
        .map_err(|_| AppError::validation("invalid item id").into())
}

fn tagged(note: Versioned<Note>) -> NoteResult {
    let version = note.version;
    NoteResult::new(note.into(), version)
}

/// Unit struct so `dump-spec` can build the spec without a database.
#[derive(Default)]
pub struct NotesEndpoints;

#[OpenApi(prefix_path = "/notes", tag = "ApiTag::Notes")]
impl NotesEndpoints {
    /// Create a note.
    #[oai(path = "/", method = "post", operation_id = "create_note")]
    async fn create(
        &self,
        notes: Data<&NoteService>,
        body: Json<CreateNoteRequest>,
    ) -> ApiResult<NoteResult> {
        let author_id: AuthorId = body
            .0
            .author_id
            .parse()
            .map_err(|_| AppError::invalid_field("authorId", "authorId must be a user id"))?;
        let note = notes
            .0
            .create(CreateNote {
                author_id,
                title: body.0.title,
                body: body.0.body,
            })
            .await?;
        Ok(tagged(note))
    }

    /// Fetch a note by id. The response carries its version in the `ETag` header
    /// (and body) — the token a subsequent edit must send back in `If-Match`.
    #[oai(path = "/:id", method = "get", operation_id = "get_note")]
    async fn get(&self, notes: Data<&NoteService>, id: Path<String>) -> ApiResult<NoteResult> {
        let note = notes.0.get(note_id(&id.0)?).await?;
        Ok(tagged(note))
    }

    /// Edit a note's title and body. Requires `If-Match` with the version the
    /// client last read; a stale value is refused with `412` and changes nothing.
    /// An archived note is refused with `409`.
    #[oai(path = "/:id", method = "patch", operation_id = "edit_note")]
    async fn edit(
        &self,
        notes: Data<&NoteService>,
        id: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
        body: Json<EditNoteRequest>,
    ) -> ApiResult<NoteResult> {
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
            .await?;
        Ok(tagged(note))
    }

    /// Delete a note. Requires `If-Match` with the version the client last read,
    /// like an edit. An archived note is refused with `409`.
    #[oai(path = "/:id", method = "delete", operation_id = "delete_note")]
    async fn delete(
        &self,
        notes: Data<&NoteService>,
        id: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
    ) -> ApiResult<Deleted> {
        let expected = if_match_version(if_match.0)?;
        notes.0.delete(note_id(&id.0)?, expected).await?;
        Ok(Deleted::Deleted)
    }

    /// Add an item to a note's checklist. Requires `If-Match` with the note's
    /// version, like an edit, and answers with the note at its new version.
    #[oai(path = "/:id/items", method = "post", operation_id = "add_note_item")]
    async fn add_item(
        &self,
        notes: Data<&NoteService>,
        id: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
        body: Json<AddItemRequest>,
    ) -> ApiResult<NoteResult> {
        let expected = if_match_version(if_match.0)?;
        let note = notes
            .0
            .add_item(note_id(&id.0)?, &body.0.text, expected)
            .await?;
        Ok(tagged(note))
    }

    /// Remove an item from a note's checklist. Requires `If-Match` with the
    /// note's version, and answers with the note at its new version.
    #[oai(
        path = "/:id/items/:item_id",
        method = "delete",
        operation_id = "remove_note_item"
    )]
    async fn remove_item(
        &self,
        notes: Data<&NoteService>,
        id: Path<String>,
        item_id: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
    ) -> ApiResult<NoteResult> {
        let expected = if_match_version(if_match.0)?;
        let note = notes
            .0
            .remove_item(note_id(&id.0)?, parse_item_id(&item_id.0)?, expected)
            .await?;
        Ok(tagged(note))
    }

    /// List notes, paginated. Most-recent first unless `sort` says otherwise:
    /// repeat it to sort by several keys, `-` prefix for descending.
    #[oai(path = "/", method = "get", operation_id = "list_notes")]
    async fn list(
        &self,
        notes: Data<&NoteService>,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
        sort: Query<Option<Vec<NoteSort>>>,
    ) -> ApiResult<Json<ListResult<NoteResponse>>> {
        let query = paged(limit.0, offset.0).sorted(sorts(sort.0));
        let page = query.page;
        let listed = notes.0.list(query).await?;
        Ok(Json(ListResult::new(listed, page)))
    }
}
