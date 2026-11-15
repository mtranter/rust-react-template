use super::models::{EntryResponse, EntrySort};
use crate::application::WidgetIndexService;
use app_web::{ApiResult, ListResult, paged, sorts};
use poem::web::Data;
use poem_openapi::{OpenApi, Tags, param::Query, payload::Json};

#[derive(Tags)]
enum ApiTag {
    WidgetIndex,
}

/// Unit struct so `dump-spec` can build the spec without a database.
#[derive(Default)]
pub struct WidgetIndexEndpoints;

#[OpenApi(prefix_path = "/widget-index", tag = "ApiTag::WidgetIndex")]
impl WidgetIndexEndpoints {
    /// List this module's copy of every root it projects, most recently updated first
    /// unless `sort` says otherwise, paginated. Answers `unavailable` until the
    /// copy has caught up with what was published before it existed.
    #[oai(path = "/", method = "get", operation_id = "list_widget_index")]
    async fn list(
        &self,
        index: Data<&WidgetIndexService>,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
        sort: Query<Option<Vec<EntrySort>>>,
    ) -> ApiResult<Json<ListResult<EntryResponse>>> {
        let query = paged(limit.0, offset.0).sorted(sorts(sort.0));
        let page = query.page;
        let found = index.0.list(query).await?;
        Ok(Json(ListResult::new(found, page)))
    }
}
