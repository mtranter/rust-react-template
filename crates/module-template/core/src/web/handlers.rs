use super::models::{CreateWidgetRequest, RenameWidgetRequest, WidgetResponse, WidgetSort};
use crate::application::{Versioned, WidgetService};
use crate::domain::{CreateWidget, RenameWidget, Widget, WidgetId};
use app_error::AppError;
use app_web::{ApiResult, ListResult, WithETag, if_match_version, paged, sorts};
use poem::web::Data;
use poem_openapi::{
    OpenApi, Tags,
    param::{Header, Path, Query},
    payload::Json,
};

type WidgetResult = WithETag<WidgetResponse>;

#[derive(Tags)]
enum ApiTag {
    Widgets,
}

fn widget_id(raw: &str) -> ApiResult<WidgetId> {
    raw.parse()
        .map_err(|_| AppError::validation("invalid widget id").into())
}

fn tagged(widget: Versioned<Widget>) -> WidgetResult {
    let version = widget.version;
    WidgetResult::new(widget.into(), version)
}

/// Unit struct so `dump-spec` can build the spec without a database.
#[derive(Default)]
pub struct WidgetsEndpoints;

#[OpenApi(prefix_path = "/widgets", tag = "ApiTag::Widgets")]
impl WidgetsEndpoints {
    /// Create a widget.
    #[oai(path = "/", method = "post")]
    async fn create(
        &self,
        widgets: Data<&WidgetService>,
        body: Json<CreateWidgetRequest>,
    ) -> ApiResult<WidgetResult> {
        let widget = widgets.0.create(CreateWidget { name: body.0.name }).await?;
        Ok(tagged(widget))
    }

    /// Fetch a widget by id. The response carries its version in the `ETag`
    /// header (and body) — the token a later rename must send back in `If-Match`.
    #[oai(path = "/:id", method = "get")]
    async fn get(
        &self,
        widgets: Data<&WidgetService>,
        id: Path<String>,
    ) -> ApiResult<WidgetResult> {
        let widget = widgets.0.get(widget_id(&id.0)?).await?;
        Ok(tagged(widget))
    }

    /// Rename a widget. Requires `If-Match` with the version the client last
    /// read; a stale value is refused with `412` and changes nothing.
    #[oai(path = "/:id", method = "patch")]
    async fn rename(
        &self,
        widgets: Data<&WidgetService>,
        id: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
        body: Json<RenameWidgetRequest>,
    ) -> ApiResult<WidgetResult> {
        let expected = if_match_version(if_match.0)?;
        let widget = widgets
            .0
            .rename(
                RenameWidget {
                    widget_id: widget_id(&id.0)?,
                    name: body.0.name,
                },
                expected,
            )
            .await?;
        Ok(tagged(widget))
    }

    /// List widgets, paginated. Most-recent first unless `sort` says otherwise:
    /// repeat it to sort by several keys, `-` prefix for descending.
    #[oai(path = "/", method = "get")]
    async fn list(
        &self,
        widgets: Data<&WidgetService>,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
        sort: Query<Option<Vec<WidgetSort>>>,
    ) -> ApiResult<Json<ListResult<WidgetResponse>>> {
        let query = paged(limit.0, offset.0).sorted(sorts(sort.0));
        let page = query.page;
        let listed = widgets.0.list(query).await?;
        Ok(Json(ListResult::new(listed, page)))
    }
}
