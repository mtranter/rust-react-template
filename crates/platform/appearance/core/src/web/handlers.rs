use super::models::{ThemeInput, ThemeResponse};
use crate::application::{ThemeService, UpdateTheme, Version};
use app_web::{ApiResult, Caller, WithETag, if_match_version};
use poem::web::Data;
use poem_openapi::{OpenApi, Tags, param::Header, payload::Json};

type ThemeResult = WithETag<ThemeResponse>;

#[derive(Tags)]
enum ApiTag {
    Appearance,
}

fn tagged(theme: ThemeResponse) -> ThemeResult {
    let version = Version::from_i64(theme.version);
    ThemeResult::new(theme, version)
}

/// Unit struct so `dump-spec` can build the spec without a database.
#[derive(Default)]
pub struct AppearanceEndpoints;

#[OpenApi(prefix_path = "/appearance", tag = "ApiTag::Appearance")]
impl AppearanceEndpoints {
    /// Fetch the organisation's theme: its overrides of the app's CSS variables (names without
    /// `--`) for the light scheme, the dark scheme, and those both share (radius, fonts,
    /// shadows). Version `0` with empty maps means the defaults, never changed. The version is
    /// also in the `ETag` header, which a change must send back in `If-Match`. Public: no token
    /// is needed.
    #[oai(path = "/theme", method = "get", operation_id = "get_theme")]
    async fn get(&self, themes: Data<&ThemeService>) -> ApiResult<ThemeResult> {
        Ok(tagged(themes.0.get().await?.into()))
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
        themes: Data<&ThemeService>,
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
        let theme = themes.0.update(&caller, cmd, expected).await?;
        Ok(tagged(Some(theme).into()))
    }
}
