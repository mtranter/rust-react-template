//! How an MCP client learns where to get a token for [`PATH`](super::PATH):
//! the server's Protected Resource Metadata (RFC 9728), naming the issuer of
//! the tokens it takes as its authorization server, as the MCP authorization
//! spec asks of a resource server.

use crate::{ApiError, Claims, PublicUrl};
use app_error::AppError;
use poem::web::Json;
use poem::{Endpoint, IntoResponse, Request, Response};
use serde::Serialize;

/// Where the metadata is served: at it, and at it followed by the resource's
/// path, which MCP clients try first.
pub const METADATA_PATH: &str = "/.well-known/oauth-protected-resource";

/// What the server tells MCP clients about signing in.
#[derive(Clone, Debug, Default)]
pub struct Discovery {
    public_url: PublicUrl,
    authorization_server: Option<String>,
    scope: Option<String>,
}

impl Discovery {
    /// `authorization_server` is the issuer of the tokens the server takes
    /// (`APP_JWT_ISSUER`), exactly as configured. Without one (development's
    /// shared secret) there is nowhere to send a client, so there is no
    /// metadata.
    pub fn new(public_url: PublicUrl, authorization_server: Option<String>) -> Self {
        Self {
            public_url,
            authorization_server,
            scope: None,
        }
    }

    /// Takes only tokens granting `scope` (`APP_MCP_SCOPE`, a Cognito
    /// resource server's custom scope) or whose `aud` is the MCP resource's
    /// URL (RFC 8707), and lists it in the metadata. `None` takes any valid
    /// token.
    pub fn with_scope(self, scope: Option<String>) -> Self {
        Self { scope, ..self }
    }

    fn resource(&self, request: &Request) -> String {
        format!("{}{}", self.public_url.of(request), super::PATH)
    }

    /// Whether `claims` are of a token meant for the MCP resource.
    pub(super) fn admits(&self, request: &Request, claims: &Claims) -> bool {
        let Some(scope) = &self.scope else {
            return true;
        };
        claims.grants(scope)
            || claims
                .aud
                .as_ref()
                .is_some_and(|aud| aud.contains(&self.resource(request)))
    }

    fn metadata_url(&self, request: &Request) -> String {
        format!(
            "{}{METADATA_PATH}{}",
            self.public_url.of(request),
            super::PATH
        )
    }

    /// The `WWW-Authenticate` challenge refusing `request` for a token not
    /// granting the scope (RFC 6750's step-up), with the metadata's URL
    /// where there is metadata.
    pub(super) fn insufficient_scope(&self, request: &Request) -> String {
        let mut challenge = format!(
            r#"Bearer error="insufficient_scope", scope="{}""#,
            self.scope.as_deref().unwrap_or_default()
        );
        if self.authorization_server.is_some() {
            challenge.push_str(&format!(
                r#", resource_metadata="{}""#,
                self.metadata_url(request)
            ));
        }
        challenge
    }

    /// The Protected Resource Metadata endpoint, for [`METADATA_PATH`] and
    /// the path naming the resource.
    pub fn metadata(&self) -> Metadata {
        Metadata(self.clone())
    }

    /// The `WWW-Authenticate` challenge refusing `request` for want of a
    /// token, pointing at the metadata; none where there is none.
    pub(super) fn challenge(&self, request: &Request) -> Option<String> {
        self.authorization_server.as_ref()?;
        let scope = self
            .scope
            .as_ref()
            .map(|scope| format!(r#"scope="{scope}", "#))
            .unwrap_or_default();
        Some(format!(
            r#"Bearer {scope}resource_metadata="{}""#,
            self.metadata_url(request)
        ))
    }

    fn metadata_of(&self, request: &Request) -> Option<ResourceMetadata> {
        let authorization_server = self.authorization_server.clone()?;
        Some(ResourceMetadata {
            resource: self.resource(request),
            authorization_servers: [authorization_server],
            bearer_methods_supported: ["header"],
            scopes_supported: self.scope.clone().map(|scope| [scope]),
        })
    }
}

/// RFC 9728's fields the server has values for: scopes only when it asks
/// for one.
#[derive(Serialize)]
struct ResourceMetadata {
    resource: String,
    authorization_servers: [String; 1],
    bearer_methods_supported: [&'static str; 1],
    #[serde(skip_serializing_if = "Option::is_none")]
    scopes_supported: Option<[String; 1]>,
}

#[derive(Clone)]
pub struct Metadata(Discovery);

impl Endpoint for Metadata {
    type Output = Response;

    async fn call(&self, request: Request) -> poem::Result<Response> {
        Ok(match self.0.metadata_of(&request) {
            Some(metadata) => Json(metadata).into_response(),
            None => {
                ApiError::from(AppError::not_found("protected resource metadata")).into_response()
            }
        })
    }
}
