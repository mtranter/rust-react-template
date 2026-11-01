use super::models::{NoteSummaryResponse, NoteSummarySort};
use crate::application::NoteSearchService;
use app_web::{ApiResult, ListResult, paged, sorts};
use poem::web::Data;
use poem_openapi::{OpenApi, Tags, param::Query, payload::Json};

#[derive(Tags)]
enum ApiTag {
    NoteSearch,
}

/// Unit struct so `dump-spec` can build the spec without a database.
#[derive(Default)]
pub struct NoteSearchEndpoints;

#[OpenApi(prefix_path = "/note-search", tag = "ApiTag::NoteSearch")]
impl NoteSearchEndpoints {
    /// Search notes, each with its author's name, paginated. `q` matches
    /// case-insensitively anywhere in a note's title or its author's name; omit
    /// it to list every note. Most-recent first unless `sort` says otherwise:
    /// repeat it to sort by several keys, `-` prefix for descending.
    #[oai(path = "/", method = "get")]
    async fn search(
        &self,
        search: Data<&NoteSearchService>,
        q: Query<Option<String>>,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
        sort: Query<Option<Vec<NoteSummarySort>>>,
    ) -> ApiResult<Json<ListResult<NoteSummaryResponse>>> {
        let query = paged(limit.0, offset.0).sorted(sorts(sort.0));
        let page = query.page;
        let found = search.0.search(q.0, query).await?;
        Ok(Json(ListResult::new(found, page)))
    }
}
