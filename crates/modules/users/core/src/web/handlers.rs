use super::models::{ChangeDisplayNameRequest, CreateUserRequest, UserResponse, UserSort};
use crate::application::{UserService, Versioned};
use crate::domain::{ChangeDisplayName, RegisterUser, User, UserId};
use app_error::AppError;
use app_web::{ApiResult, JwtAuth, ListResult, WithETag, if_match_version, paged, sorts};
use poem::web::Data;
use poem_openapi::{
    ApiResponse, OpenApi, Tags,
    param::{Header, Path, Query},
    payload::Json,
};

type UserResult = WithETag<UserResponse>;

#[derive(Tags)]
enum ApiTag {
    Users,
}

#[derive(ApiResponse)]
enum Deleted {
    #[oai(status = 204)]
    Deleted,
}

fn caller_id(auth: &JwtAuth) -> ApiResult<UserId> {
    auth.0
        .sub
        .parse()
        .map_err(|_| AppError::Unauthorized.into())
}

fn tagged(user: Versioned<User>) -> UserResult {
    let version = user.version;
    UserResult::new(user.into(), version)
}

/// A unit struct, so `dump-spec` can build the spec without a database.
#[derive(Default)]
pub struct UsersEndpoints;

#[OpenApi(prefix_path = "/users", tag = "ApiTag::Users")]
impl UsersEndpoints {
    /// Register a new user.
    #[oai(path = "/", method = "post")]
    async fn create(
        &self,
        users: Data<&UserService>,
        body: Json<CreateUserRequest>,
    ) -> ApiResult<UserResult> {
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
            .await?;
        Ok(tagged(user))
    }

    /// The currently-authenticated user (JWT Bearer guarded — spec §11). The
    /// response carries the version in the `ETag` header (and body) — the token
    /// a subsequent change must send back in `If-Match`.
    ///
    /// Declared before `/:id` so the static segment wins in routing.
    #[oai(path = "/me", method = "get")]
    async fn me(&self, users: Data<&UserService>, auth: JwtAuth) -> ApiResult<UserResult> {
        let user = users.0.get(caller_id(&auth)?).await?;
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
    ) -> ApiResult<UserResult> {
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
            .await?;
        Ok(tagged(user))
    }

    /// Delete the currently-authenticated user's account. Requires `If-Match`
    /// with the version the client last read, like a change. Their notes are
    /// archived shortly after, not in this request.
    #[oai(path = "/me", method = "delete")]
    async fn delete(
        &self,
        users: Data<&UserService>,
        auth: JwtAuth,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
    ) -> ApiResult<Deleted> {
        let expected = if_match_version(if_match.0)?;
        users.0.delete(caller_id(&auth)?, expected).await?;
        Ok(Deleted::Deleted)
    }

    /// Fetch a user by id.
    #[oai(path = "/:id", method = "get")]
    async fn get(&self, users: Data<&UserService>, id: Path<String>) -> ApiResult<UserResult> {
        let id: UserId =
            id.0.parse()
                .map_err(|_| AppError::validation("invalid user id"))?;
        let user = users.0.get(id).await?;
        Ok(tagged(user))
    }

    /// List users, paginated. Most-recent first unless `sort` says otherwise:
    /// repeat it to sort by several keys, `-` prefix for descending.
    #[oai(path = "/", method = "get")]
    async fn list(
        &self,
        users: Data<&UserService>,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
        sort: Query<Option<Vec<UserSort>>>,
    ) -> ApiResult<Json<ListResult<UserResponse>>> {
        let query = paged(limit.0, offset.0).sorted(sorts(sort.0));
        let page = query.page;
        let listed = users.0.list(query).await?;
        Ok(Json(ListResult::new(listed, page)))
    }
}
