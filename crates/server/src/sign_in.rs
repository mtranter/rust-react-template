//! `GET /auth/config`: how the embedded SPA reaches the identity provider, read
//! at runtime so one build serves any deployment. Outside `/api`, like
//! `/health`, so it is no operation, spec entry or MCP tool.

use poem::{
    Endpoint, EndpointExt, Result, get, handler,
    http::StatusCode,
    web::{Data, Json},
};
use serde::Serialize;
use std::collections::BTreeMap;

pub const PATH: &str = "/auth/config";

/// Requested whatever `APP_OIDC_SCOPES` adds.
pub const BASE_SCOPES: [&str; 3] = ["openid", "profile", "email"];

/// The SPA's sign-in settings, as `/auth/config` serves them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignIn {
    pub issuer: String,
    /// `null` when `APP_OIDC_CLIENT_ID` is unset, which only dev and test allow.
    pub client_id: Option<String>,
    /// Every scope to request: [`BASE_SCOPES`], then `APP_OIDC_SCOPES`'.
    pub scopes: Vec<String>,
    /// Added to the authorization request as they are.
    pub extra_params: BTreeMap<String, String>,
}

/// `None` (no `APP_JWT_ISSUER`) answers `404`, so the SPA skips signing in.
pub fn endpoint(sign_in: Option<SignIn>) -> impl Endpoint {
    get(config).data(sign_in)
}

#[handler]
fn config(sign_in: Data<&Option<SignIn>>) -> Result<Json<SignIn>> {
    sign_in.0.clone().map(Json).ok_or_else(|| {
        poem::Error::from_string(
            "no sign-in is configured: APP_JWT_ISSUER is unset",
            StatusCode::NOT_FOUND,
        )
    })
}
