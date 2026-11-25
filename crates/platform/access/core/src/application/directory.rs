use crate::domain::Email;
use app_error::AppResult;
use async_trait::async_trait;

/// Where an invitee's account is in the identity provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryStatus {
    /// Created, waiting for them to sign in with the temporary password and set their own.
    Pending,
    /// Pending, but the temporary password has lapsed: resending gives them a fresh one.
    Expired,
    /// They have set their password.
    Active,
    /// The provider has no such user (deleted outside the app).
    Missing,
}

/// A user the identity provider has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryUser {
    /// What tokens carry as `sub`.
    pub subject: String,
    pub status: DirectoryStatus,
}

/// What `create` finds instead of creating a user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Created {
    Created(DirectoryUser),
    /// The provider has a user with that email already: as it is now.
    Exists(DirectoryUser),
}

/// The identity provider's user store: an external system, so tests use a Fake (ADR 0004).
/// `cognito/` holds the adapter the server uses. Refusals the provider makes of the email are
/// invalid on `email`; the provider unreachable is `Unavailable`.
#[async_trait]
pub trait IdentityDirectory: Send + Sync {
    /// Creates a user who must set a password at first sign-in, and has the provider mail them.
    async fn create(&self, email: &Email, name: &str) -> AppResult<Created>;
    /// Mails a pending user a fresh temporary password.
    async fn resend(&self, email: &Email) -> AppResult<()>;
    async fn find(&self, email: &Email) -> AppResult<DirectoryUser>;
    /// Removes a user, gone already or not.
    async fn delete(&self, email: &Email) -> AppResult<()>;
}
