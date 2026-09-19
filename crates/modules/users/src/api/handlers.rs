//! The users `#[OpenApi]` operations.

use super::models::{ChangeDisplayNameRequest, CreateUserRequest, UserResponse};
use crate::application::{UserService, Versioned};
use crate::domain::{ChangeDisplayName, RegisterUser, User, UserId};
use app_error::AppError;
use app_web::{ApiResultExt, JwtAuth, Page, Pagination, WithETag, if_match_version};
use poem::web::Data;
use poem_openapi::{
    OpenApi, Tags,
    param::{Header, Path, Query},
    payload::Json,
};

/// Every single-item user read/write returns the user with its version as an
/// `ETag` header (and in the body) — the token a client's next change sends back.
type UserResult = WithETag<UserResponse>;

#[derive(Tags)]
enum ApiTag {
    Users,
}

/// The authenticated caller's id, from the JWT `sub` claim.
fn caller_id(auth: &JwtAuth) -> poem::Result<UserId> {
    auth.0.sub.parse().map_err(|_| AppError::Unauthorized).api()
}

/// A saved/loaded user as its ETag-carrying response (version in header and body).
fn tagged(user: Versioned<User>) -> UserResult {
    let version = user.version;
    UserResult::new(user.into(), version)
}

/// The users module API. Unit struct — no state. The service is injected as
/// request data (`Data<&UserService>`), wired by the module's `provider`.
pub struct UsersApi;

#[OpenApi(prefix_path = "/users", tag = "ApiTag::Users")]
impl UsersApi {
    /// Register a new user.
    #[oai(path = "/", method = "post")]
    async fn create(
        &self,
        users: Data<&UserService>,
        body: Json<CreateUserRequest>,
    ) -> poem::Result<UserResult> {
        let CreateUserRequest {
            email,
            display_name,
        } = body.0;
        let user = users
            .0
            .register(RegisterUser {
                email,
                display_name,
            })
            .await
            .api()?;
        Ok(tagged(user))
    }

    /// The currently-authenticated user (JWT Bearer guarded — spec §11). The
    /// response carries the version in the `ETag` header (and body) — the token
    /// a subsequent change must send back in `If-Match`.
    ///
    /// Declared before `/:id` so the static segment wins in routing.
    #[oai(path = "/me", method = "get")]
    async fn me(&self, users: Data<&UserService>, auth: JwtAuth) -> poem::Result<UserResult> {
        let user = users.0.get(caller_id(&auth)?).await.api()?;
        Ok(tagged(user))
    }

    /// Change the currently-authenticated user's display name. Requires `If-Match`
    /// with the version the client last read; a stale value is refused with `412`
    /// and changes nothing (e.g. a change already made in another tab).
    #[oai(path = "/me", method = "patch")]
    async fn change_display_name(
        &self,
        users: Data<&UserService>,
        auth: JwtAuth,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
        body: Json<ChangeDisplayNameRequest>,
    ) -> poem::Result<UserResult> {
        let expected = if_match_version(if_match.0)?;
        let user = users
            .0
            .change_display_name(
                ChangeDisplayName {
                    user_id: caller_id(&auth)?,
                    display_name: body.0.display_name,
                },
                expected,
            )
            .await
            .api()?;
        Ok(tagged(user))
    }

    /// Fetch a user by id.
    #[oai(path = "/:id", method = "get")]
    async fn get(&self, users: Data<&UserService>, id: Path<String>) -> poem::Result<UserResult> {
        let id: UserId =
            id.0.parse()
                .map_err(|_| AppError::validation("invalid user id"))
                .api()?;
        let user = users.0.get(id).await.api()?;
        Ok(tagged(user))
    }

    /// List users, most-recent first (paginated).
    #[oai(path = "/", method = "get")]
    async fn list(
        &self,
        users: Data<&UserService>,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
    ) -> poem::Result<Json<Page<UserResponse>>> {
        let page = Pagination::new(limit.0, offset.0);
        let (list, total) = users.0.list(page.limit, page.offset).await.api()?;
        let items = list.into_iter().map(UserResponse::from).collect();
        Ok(Json(Page::new(items, total, page)))
    }
}
