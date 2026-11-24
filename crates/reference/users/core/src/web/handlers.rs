use super::models::{ChangeDisplayNameRequest, CreateUserRequest, UserResponse, UserSort};
use crate::application::{UserService, Versioned};
use crate::domain::{ChangeDisplayName, RegisterUser, User, UserId};
use app_error::AppError;
use app_web::{ApiResult, Caller, ListResult, WithETag, if_match_version, paged, sorts};
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

fn caller_id(caller: &Caller) -> ApiResult<UserId> {
    caller
        .id()
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| AppError::Unauthorized.into())
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
    /// Register a new user. Public: no token is needed.
    #[oai(path = "/", method = "post", operation_id = "register_user")]
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

    /// The currently-authenticated user. The
    /// response carries the version in the `ETag` header (and body) — the token
    /// a subsequent change must send back in `If-Match`.
    ///
    /// Declared before `/:id` so the static segment wins in routing.
    #[oai(path = "/me", method = "get", operation_id = "get_me")]
    async fn me(&self, users: Data<&UserService>, caller: Caller) -> ApiResult<UserResult> {
        let user = users.0.get(&caller, caller_id(&caller)?).await?;
        Ok(tagged(user))
    }

    /// Change the currently-authenticated user's display name. Requires `If-Match`
    /// with the version the client last read; a stale value is refused with `412`
    /// and changes nothing (e.g. a change already made in another tab).
    #[oai(path = "/me", method = "patch", operation_id = "rename_me")]
    async fn change_display_name(
        &self,
        users: Data<&UserService>,
        caller: Caller,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
        body: Json<ChangeDisplayNameRequest>,
    ) -> ApiResult<UserResult> {
        let expected = if_match_version(if_match.0)?;
        let user = users
            .0
            .change_display_name(
                &caller,
                ChangeDisplayName {
                    user_id: caller_id(&caller)?,
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
    #[oai(path = "/me", method = "delete", operation_id = "delete_me")]
    async fn delete(
        &self,
        users: Data<&UserService>,
        caller: Caller,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
    ) -> ApiResult<Deleted> {
        let expected = if_match_version(if_match.0)?;
        users
            .0
            .delete(&caller, caller_id(&caller)?, expected)
            .await?;
        Ok(Deleted::Deleted)
    }

    /// Fetch a user by id.
    #[oai(path = "/:id", method = "get", operation_id = "get_user")]
    async fn get(
        &self,
        users: Data<&UserService>,
        caller: Caller,
        id: Path<String>,
    ) -> ApiResult<UserResult> {
        let id: UserId =
            id.0.parse()
                .map_err(|_| AppError::validation("invalid user id"))?;
        let user = users.0.get(&caller, id).await?;
        Ok(tagged(user))
    }

    /// List users, paginated. Most-recent first unless `sort` says otherwise:
    /// repeat it to sort by several keys, `-` prefix for descending.
    #[oai(path = "/", method = "get", operation_id = "list_users")]
    async fn list(
        &self,
        users: Data<&UserService>,
        caller: Caller,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
        sort: Query<Option<Vec<UserSort>>>,
    ) -> ApiResult<Json<ListResult<UserResponse>>> {
        let query = paged(limit.0, offset.0).sorted(sorts(sort.0));
        let page = query.page;
        let listed = users.0.list(&caller, query).await?;
        Ok(Json(ListResult::new(listed, page)))
    }
}
