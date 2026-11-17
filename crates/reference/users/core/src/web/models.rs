use crate::application::{UserSortKey, Versioned};
use crate::domain::User;
use app_persistence::{Direction, Sort};
use app_web::SortParam;
use poem_openapi::{Enum, Object};

/// Request body for creating a user.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct CreateUserRequest {
    pub email: String,
    pub display_name: String,
}

/// Request body for changing the current user's display name. Carries no
/// version: the concurrency token travels only in the `If-Match` header.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct ChangeDisplayNameRequest {
    pub display_name: String,
}

/// A user as exposed over HTTP. Ids and timestamps are strings so the generated
/// client stays dependency-free (uuid/RFC3339 as plain `string`).
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub created_at: String,
    /// Optimistic-concurrency version. Echo it back in `If-Match` to change this
    /// user; a stale value is refused with `412 Precondition Failed`. Also
    /// mirrored in the `ETag` response header for single-item reads.
    pub version: String,
}

impl From<Versioned<User>> for UserResponse {
    fn from(versioned: Versioned<User>) -> Self {
        let Versioned { entity: u, version } = versioned;
        Self {
            id: u.id().to_string(),
            email: u.email().to_string(),
            display_name: u.display_name().to_string(),
            created_at: u.created_at().to_rfc3339(),
            version: version.as_i64().to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Enum)]
pub enum UserSort {
    #[oai(rename = "email")]
    EmailAsc,
    #[oai(rename = "-email")]
    EmailDesc,
    #[oai(rename = "displayName")]
    DisplayNameAsc,
    #[oai(rename = "-displayName")]
    DisplayNameDesc,
    #[oai(rename = "createdAt")]
    CreatedAtAsc,
    #[oai(rename = "-createdAt")]
    CreatedAtDesc,
}

impl SortParam for UserSort {
    type Key = UserSortKey;

    fn sort(self) -> Sort<UserSortKey> {
        let (key, direction) = match self {
            Self::EmailAsc => (UserSortKey::Email, Direction::Asc),
            Self::EmailDesc => (UserSortKey::Email, Direction::Desc),
            Self::DisplayNameAsc => (UserSortKey::DisplayName, Direction::Asc),
            Self::DisplayNameDesc => (UserSortKey::DisplayName, Direction::Desc),
            Self::CreatedAtAsc => (UserSortKey::CreatedAt, Direction::Asc),
            Self::CreatedAtDesc => (UserSortKey::CreatedAt, Direction::Desc),
        };
        Sort { key, direction }
    }
}
