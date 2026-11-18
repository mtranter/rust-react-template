use crate::application::SignIns;
use app_authz::{Authorizer, Principal};
use app_error::AppResult;
use app_web::{Claims, PrincipalResolver};
use std::sync::Arc;

/// Resolves a token's Principal as the [`Authorizer`] does, recording the subject as seen.
pub struct RecordingSignIns {
    pub(crate) sign_ins: SignIns,
    pub(crate) authorizer: Arc<Authorizer>,
}

#[async_trait::async_trait]
impl PrincipalResolver for RecordingSignIns {
    async fn resolve(&self, claims: &Claims) -> AppResult<Principal> {
        // Failing to record a sighting costs an admin a row's freshness, not the caller a request.
        if let Err(err) = self
            .sign_ins
            .seen(&claims.sub, claims.name.clone(), claims.email.clone())
            .await
        {
            tracing::warn!(error = %err, "a sign-in went unrecorded");
        }
        self.authorizer.resolve(claims).await
    }
}
