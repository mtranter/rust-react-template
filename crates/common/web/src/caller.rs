//! A Module's gate (ADR 0040): its operations take `Caller<G>`, which yields the [`Principal`]
//! only once the token is valid and the gate, `module:<G::MODULE>` / `use`, allows its roles.

use crate::auth::bearer_token;
use crate::{ApiError, JwtAuth};
use app_authz::{Authorizer, Principal};
use app_error::AppError;
use poem::{IntoResponse, Request, RequestBody};
use poem_openapi::registry::Registry;
use poem_openapi::{ApiExtractor, ApiExtractorType, ExtractParamOptions};
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;

/// Names a Module for its `Caller`: `pub struct Widgets; impl Gate for Widgets { const MODULE: &str = "widgets"; }`.
pub trait Gate: Send + Sync + 'static {
    const MODULE: &'static str;
}

/// The Principal acting, admitted by `G`'s gate. No token is `401`; a token whose roles the gate
/// doesn't allow is `403`. In the spec it is the same bearer requirement as [`JwtAuth`].
pub struct Caller<G> {
    principal: Principal,
    token: String,
    gate: PhantomData<fn() -> G>,
}

impl<G> Caller<G> {
    pub fn principal(&self) -> &Principal {
        &self.principal
    }

    /// The bearer token the request presented, for calls made as the caller within it (the
    /// assistant's tools). Never kept beyond the request, nor logged.
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl<G> Deref for Caller<G> {
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

#[derive(Debug, thiserror::Error)]
#[error("no `Arc<Authorizer>` in request data: the server provides it beside the `JwtVerifier`")]
struct NoAuthorizer;

fn refusal(err: AppError) -> poem::Error {
    poem::Error::from_response(ApiError::from(err).into_response())
}

impl<'a, G: Gate> ApiExtractor<'a> for Caller<G> {
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
        let authorizer = req
            .data::<Arc<Authorizer>>()
            .ok_or_else(|| refusal(AppError::internal(NoAuthorizer)))?;
        if !authorizer.allows_use_of(&principal, G::MODULE) {
            return Err(refusal(AppError::Forbidden));
        }
        let token = bearer_token(req).unwrap_or_default().to_owned();
        Ok(Self {
            principal,
            token,
            gate: PhantomData,
        })
    }
}
