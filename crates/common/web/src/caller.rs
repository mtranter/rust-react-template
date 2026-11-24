use crate::JwtAuth;
use crate::auth::bearer_token;
use app_authz::Principal;
use poem::{Request, RequestBody};
use poem_openapi::registry::Registry;
use poem_openapi::{ApiExtractor, ApiExtractorType, ExtractParamOptions};
use std::ops::Deref;

/// The Principal a guarded operation acts for: a valid token's, else `401`. It checks no role:
/// the use case asks for its action (ADR 0047). In the spec it is the same bearer requirement as
/// [`JwtAuth`].
pub struct Caller {
    principal: Principal,
    token: String,
}

impl Caller {
    pub fn principal(&self) -> &Principal {
        &self.principal
    }

    /// The bearer token the request presented, for calls made as the caller within it (the
    /// assistant's tools). Never kept beyond the request, nor logged.
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl Deref for Caller {
    type Target = Principal;

    fn deref(&self) -> &Principal {
        &self.principal
    }
}

/// An operation and the action it performs: a row of a Module's `OPERATIONS`, what an admin
/// granting the action is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Performs {
    pub operation: &'static str,
    pub resource: &'static str,
    pub action: &'static str,
}

impl Performs {
    pub const fn new(
        operation: &'static str,
        resource: &'static str,
        action: &'static str,
    ) -> Self {
        Self {
            operation,
            resource,
            action,
        }
    }
}

impl<'a> ApiExtractor<'a> for Caller {
    const TYPES: &'static [ApiExtractorType] = &[ApiExtractorType::SecurityScheme];

    type ParamType = ();
    type ParamRawType = ();

    fn register(registry: &mut Registry) {
        JwtAuth::register(registry);
    }

    fn security_schemes() -> Vec<&'static str> {
        JwtAuth::security_schemes()
    }

    async fn from_request(
        req: &'a Request,
        body: &mut RequestBody,
        param_opts: ExtractParamOptions<Self::ParamType>,
    ) -> poem::Result<Self> {
        let JwtAuth(principal) = JwtAuth::from_request(req, body, param_opts).await?;
        let token = bearer_token(req).unwrap_or_default().to_owned();
        Ok(Self { principal, token })
    }
}
