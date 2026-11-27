use super::models::{AppVisibilityInput, AppVisibilityResponse, ThemeInput, ThemeResponse};
use crate::UiSettings;
use crate::application::{UpdateTheme, Version};
use app_web::{ApiResult, Caller, WithETag, if_match_version};
use poem::web::Data;
use poem_openapi::{OpenApi, Tags, param::Header, payload::Json};

type ThemeResult = WithETag<ThemeResponse>;
type AppVisibilityResult = WithETag<AppVisibilityResponse>;

#[derive(Tags)]
enum ApiTag {
    UiSettings,
}

fn tagged(theme: ThemeResponse) -> ThemeResult {
    let version = Version::from_i64(theme.version);
    ThemeResult::new(theme, version)
}

fn tagged_visibility(visibility: AppVisibilityResponse) -> AppVisibilityResult {
    let version = Version::from_i64(visibility.version);
    AppVisibilityResult::new(visibility, version)
}

/// Unit struct so `dump-spec` can build the spec without a database.
#[derive(Default)]
pub struct UiSettingsEndpoints;

#[OpenApi(prefix_path = "/ui-settings", tag = "ApiTag::UiSettings")]
impl UiSettingsEndpoints {
    /// Fetch the organisation's theme: its overrides of the app's CSS variables (names without
    /// `--`) for the light scheme, the dark scheme, and those both share (radius, fonts,
    /// shadows). Version `0` with empty maps means the defaults, never changed. The version is
    /// also in the `ETag` header, which a change must send back in `If-Match`. Public: no token
    /// is needed.
    #[oai(path = "/theme", method = "get", operation_id = "get_theme")]
    async fn get(&self, settings: Data<&UiSettings>) -> ApiResult<ThemeResult> {
        Ok(tagged(settings.0.themes.get().await?.into()))
    }

    /// Replace the organisation's theme with these overrides of the app's CSS variables; empty
    /// maps reset it to the defaults. Only known variable names are accepted, and values must
    /// be plain CSS values (no `;`, braces, angle brackets, backslashes or `url(`): a bad one is
    /// `400` naming it (`light.primary`). Requires `If-Match` with the version last read (`0`
    /// if never saved); a stale one is refused `412` and changes nothing. Admins only: anyone
    /// else is refused `403`.
    #[oai(path = "/theme", method = "put", operation_id = "update_theme")]
    async fn update(
        &self,
        settings: Data<&UiSettings>,
        caller: Caller,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
        body: Json<ThemeInput>,
    ) -> ApiResult<ThemeResult> {
        let expected = if_match_version(if_match.0)?;
        let ThemeInput {
            light,
            dark,
            shared,
        } = body.0;
        let cmd = UpdateTheme {
            light,
            dark,
            shared,
        };
        let theme = settings.0.themes.update(&caller, cmd, expected).await?;
        Ok(tagged(Some(theme).into()))
    }

    /// Fetch which apps the organisation hides from its web app: the ids of their Capabilities
    /// (`crm`). Hiding is only that: the apps' operations, and their tools, still answer.
    /// Version `0` with none hidden means never changed; the version is also in the `ETag`
    /// header, which a change must send back in `If-Match`. Any signed-in caller may read it.
    #[oai(
        path = "/app-visibility",
        method = "get",
        operation_id = "get_app_visibility"
    )]
    async fn get_apps(
        &self,
        settings: Data<&UiSettings>,
        _caller: Caller,
    ) -> ApiResult<AppVisibilityResult> {
        Ok(tagged_visibility(settings.0.apps.get().await?.into()))
    }

    /// Choose which apps the organisation hides from its web app, by their ids; an empty list
    /// shows every app. Hiding doesn't stop anyone using an app's operations. Requires
    /// `If-Match` with the version last read (`0` if never saved); a stale one is refused `412`
    /// and changes nothing. Admins only: anyone else is refused `403`.
    #[oai(
        path = "/app-visibility",
        method = "put",
        operation_id = "update_app_visibility"
    )]
    async fn update_apps(
        &self,
        settings: Data<&UiSettings>,
        caller: Caller,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
        body: Json<AppVisibilityInput>,
    ) -> ApiResult<AppVisibilityResult> {
        let expected = if_match_version(if_match.0)?;
        let visibility = settings
            .0
            .apps
            .update(&caller, body.0.hidden, expected)
            .await?;
        Ok(tagged_visibility(Some(visibility).into()))
    }
}
