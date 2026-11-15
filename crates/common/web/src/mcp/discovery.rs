//! How an MCP client learns where to get a token for [`PATH`](super::PATH):
//! the server's Protected Resource Metadata (RFC 9728), naming the issuer of
//! the tokens it takes as its authorization server, as the MCP authorization
//! spec asks of a resource server.

use crate::{ApiError, PublicUrl};
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
        }
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
        Some(format!(
            r#"Bearer resource_metadata="{}{METADATA_PATH}{}""#,
            self.public_url.of(request),
            super::PATH
        ))
    }

    fn metadata_of(&self, request: &Request) -> Option<ResourceMetadata> {
        let authorization_server = self.authorization_server.clone()?;
        Some(ResourceMetadata {
            resource: format!("{}{}", self.public_url.of(request), super::PATH),
            authorization_servers: [authorization_server],
            bearer_methods_supported: ["header"],
        })
    }
}

/// RFC 9728's fields the server has values for: it has no scopes.
#[derive(Serialize)]
struct ResourceMetadata {
    resource: String,
    authorization_servers: [String; 1],
    bearer_methods_supported: [&'static str; 1],
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
