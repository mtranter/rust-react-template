//! The `IdentityDirectory` adapter the server uses: Amazon Cognito user pools, the only code that
//! knows how users are created there (Spec 0012).

use crate::application::{Created, DirectoryStatus, DirectoryUser, IdentityDirectory};
use crate::domain::Email;
use app_error::{AppError, AppResult};
use async_trait::async_trait;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_cognitoidentityprovider::Client;
use aws_sdk_cognitoidentityprovider::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_cognitoidentityprovider::types::{
    AttributeType, DeliveryMediumType, MessageActionType, UserStatusType,
};
use aws_smithy_http_client::{Builder, tls};
use chrono::{DateTime, TimeDelta, Utc};
use tokio::sync::OnceCell;

/// What Cognito gives a pool that doesn't set it.
const DEFAULT_TEMPORARY_PASSWORD_DAYS: i64 = 7;

pub struct CognitoDirectory {
    pool_id: String,
    region: String,
    endpoint: Option<String>,
    client: OnceCell<Client>,
    temporary_password_days: OnceCell<i64>,
}

impl CognitoDirectory {
    /// `pool_id` is `<region>_<id>`, which names its region. `endpoint` replaces Cognito's own,
    /// for a stand-in such as `fake-oidc`'s. Credentials come from the default chain, read at
    /// the first call, so building one does no I/O.
    pub fn new(pool_id: &str, endpoint: Option<&str>) -> Result<Self, String> {
        let region = pool_id
            .split_once('_')
            .map(|(region, _)| region.to_owned())
            .filter(|region| !region.is_empty())
            .ok_or_else(|| format!("`{pool_id}` isn't a user pool id (`<region>_<id>`)"))?;
        Ok(Self {
            pool_id: pool_id.to_owned(),
            region,
            endpoint: endpoint.map(str::to_owned),
            client: OnceCell::new(),
            temporary_password_days: OnceCell::new(),
        })
    }

    async fn client(&self) -> &Client {
        self.client
            .get_or_init(|| async {
                let http = Builder::new()
                    .tls_provider(tls::Provider::Rustls(
                        tls::rustls_provider::CryptoMode::Ring,
                    ))
                    .build_https();
                let mut config = aws_config::defaults(BehaviorVersion::latest())
                    .http_client(http)
                    .region(Region::new(self.region.clone()));
                if let Some(endpoint) = &self.endpoint {
                    config = config.endpoint_url(endpoint);
                }
                Client::new(&config.load().await)
            })
            .await
    }

    async fn temporary_password_days(&self) -> i64 {
        *self
            .temporary_password_days
            .get_or_init(|| async {
                let described = self
                    .client()
                    .await
                    .describe_user_pool()
                    .user_pool_id(&self.pool_id)
                    .send()
                    .await;
                match described {
                    Ok(out) => out
                        .user_pool()
                        .and_then(|pool| pool.policies())
                        .and_then(|policies| policies.password_policy())
                        .map(|policy| policy.temporary_password_validity_days())
                        .filter(|days| *days > 0)
                        .map_or(DEFAULT_TEMPORARY_PASSWORD_DAYS, i64::from),
                    Err(err) => {
                        tracing::warn!(error = %describe(&err), "the user pool's temporary password validity went unread");
                        DEFAULT_TEMPORARY_PASSWORD_DAYS
                    }
                }
            })
            .await
    }

    async fn user_of(
        &self,
        subject: Option<String>,
        status: Option<&UserStatusType>,
        modified: Option<DateTime<Utc>>,
    ) -> AppResult<DirectoryUser> {
        let subject = subject.ok_or_else(|| unexpected("a user without a `sub`"))?;
        let status = match status {
            Some(UserStatusType::ForceChangePassword) => {
                let days = self.temporary_password_days().await;
                match modified {
                    Some(at) if Utc::now() - at > TimeDelta::days(days) => DirectoryStatus::Expired,
                    _ => DirectoryStatus::Pending,
                }
            }
            _ => DirectoryStatus::Active,
        };
        Ok(DirectoryUser { subject, status })
    }

    async fn create_as(&self, email: &Email, name: &str, resend: bool) -> AppResult<Created> {
        let mut attributes = vec![
            attribute("email", email.as_str()),
            attribute("email_verified", "true"),
        ];
        if !name.is_empty() {
            attributes.push(attribute("name", name));
        }
        let mut request = self
            .client()
            .await
            .admin_create_user()
            .user_pool_id(&self.pool_id)
            .username(email.as_str())
            .desired_delivery_mediums(DeliveryMediumType::Email)
            .set_user_attributes(Some(attributes));
        if resend {
            request = request.message_action(MessageActionType::Resend);
        }
        match request.send().await {
            Ok(out) => {
                let user = out.user().ok_or_else(|| unexpected("no user created"))?;
                let subject = user
                    .attributes()
                    .iter()
                    .find(|a| a.name() == "sub")
                    .and_then(|a| a.value())
                    .map(str::to_owned);
                Ok(Created::Created(
                    self.user_of(
                        subject,
                        user.user_status(),
                        modified(user.user_last_modified_date()),
                    )
                    .await?,
                ))
            }
            Err(err) if err.code() == Some("UsernameExistsException") => {
                Ok(Created::Exists(self.find(email).await?))
            }
            Err(err) => Err(refused(&err)),
        }
    }
}

fn attribute(name: &str, value: &str) -> AttributeType {
    AttributeType::builder()
        .name(name)
        .value(value)
        .build()
        .expect("a name is set")
}

fn modified(
    at: Option<&aws_sdk_cognitoidentityprovider::primitives::DateTime>,
) -> Option<DateTime<Utc>> {
    at.and_then(|at| DateTime::from_timestamp(at.secs(), at.subsec_nanos()))
}

fn describe<E: ProvideErrorMetadata + std::error::Error + 'static, R: std::fmt::Debug>(
    err: &SdkError<E, R>,
) -> String {
    aws_sdk_cognitoidentityprovider::error::DisplayErrorContext(err).to_string()
}

fn unexpected(what: &str) -> AppError {
    AppError::internal(std::io::Error::other(format!("Cognito answered {what}")))
}

/// Cognito's refusals of the email are the caller's to fix; failing to reach it, or a
/// throttle, is worth retrying; anything else is ours.
fn refused<E, R>(err: &SdkError<E, R>) -> AppError
where
    E: ProvideErrorMetadata + std::error::Error + Send + Sync + 'static,
    R: std::fmt::Debug,
{
    match err.code() {
        Some("InvalidParameterException") => AppError::invalid_field(
            "email",
            err.message()
                .unwrap_or("the identity provider refused the email")
                .to_owned(),
        ),
        Some(
            "TooManyRequestsException" | "LimitExceededException" | "CodeDeliveryFailureException",
        ) => AppError::unavailable(format!(
            "the identity provider is refusing for now: {}",
            err.message().unwrap_or("try again later")
        )),
        _ if matches!(
            err,
            SdkError::DispatchFailure(_) | SdkError::TimeoutError(_)
        ) =>
        {
            tracing::warn!(error = %describe(err), "Cognito unreachable");
            AppError::unavailable("the identity provider can't be reached: try again later")
        }
        _ => {
            tracing::error!(error = %describe(err), "Cognito refused");
            AppError::internal(std::io::Error::other(describe(err)))
        }
    }
}

#[async_trait]
impl IdentityDirectory for CognitoDirectory {
    async fn create(&self, email: &Email, name: &str) -> AppResult<Created> {
        self.create_as(email, name, false).await
    }

    async fn resend(&self, email: &Email) -> AppResult<()> {
        self.create_as(email, "", true).await.map(|_| ())
    }

    async fn find(&self, email: &Email) -> AppResult<DirectoryUser> {
        let found = self
            .client()
            .await
            .admin_get_user()
            .user_pool_id(&self.pool_id)
            .username(email.as_str())
            .send()
            .await;
        match found {
            Ok(out) => {
                let subject = out
                    .user_attributes()
                    .iter()
                    .find(|a| a.name() == "sub")
                    .and_then(|a| a.value())
                    .map(str::to_owned);
                self.user_of(
                    subject,
                    out.user_status(),
                    modified(out.user_last_modified_date()),
                )
                .await
            }
            Err(err) if err.code() == Some("UserNotFoundException") => Ok(DirectoryUser {
                subject: String::new(),
                status: DirectoryStatus::Missing,
            }),
            Err(err) => Err(refused(&err)),
        }
    }

    async fn delete(&self, email: &Email) -> AppResult<()> {
        let deleted = self
            .client()
            .await
            .admin_delete_user()
            .user_pool_id(&self.pool_id)
            .username(email.as_str())
            .send()
            .await;
        match deleted {
            Ok(_) => Ok(()),
            Err(err) if err.code() == Some("UserNotFoundException") => Ok(()),
            Err(err) => Err(refused(&err)),
        }
    }
}
