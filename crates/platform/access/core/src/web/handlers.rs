use super::Caller;
use super::models::{CatalogResponse, GrantRequest, GrantResponse, GrantSort};
use crate::application::{AccessService, GrantFilter, Versioned};
use crate::domain::{Grant, GrantAccess, GrantId};
use app_error::AppError;
use app_web::{ApiResult, ListResult, WithETag, if_match_version, paged, sorts};
use poem::web::Data;
use poem_openapi::{
    ApiResponse, OpenApi, Tags,
    param::{Header, Path, Query},
    payload::Json,
};

#[derive(Tags)]
enum ApiTag {
    Access,
}

#[derive(ApiResponse)]
enum Revoked {
    #[oai(status = 204)]
    Revoked,
}

fn grant_id(raw: &str) -> ApiResult<GrantId> {
    raw.parse()
        .map_err(|_| AppError::validation("invalid grant id").into())
}

fn tagged(grant: Versioned<Grant>) -> WithETag<GrantResponse> {
    let version = grant.version;
    WithETag::new(grant.into(), version)
}

/// Unit struct so `dump-spec` can build the spec without a database.
#[derive(Default)]
pub struct AccessEndpoints;

#[OpenApi(prefix_path = "/access", tag = "ApiTag::Access")]
impl AccessEndpoints {
    /// List who may do what: each grant of an action to a role, within a scope. Filter by
    /// `role`, `resource` or `action` (exact), paginated, ordered by resource, action and role
    /// unless `sort` says otherwise (repeat it to sort by several keys, `-` prefix for
    /// descending). A grant marked `stale` names an action or scope its module no longer has,
    /// and allows nothing. `403` unless the caller is an `admin`, or holds a role granted it.
    #[oai(path = "/grants", method = "get", operation_id = "list_grants")]
    #[allow(clippy::too_many_arguments)]
    async fn list(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        role: Query<Option<String>>,
        resource: Query<Option<String>>,
        action: Query<Option<String>>,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
        sort: Query<Option<Vec<GrantSort>>>,
    ) -> ApiResult<Json<ListResult<GrantResponse>>> {
        let query = paged(limit.0, offset.0).sorted(sorts(sort.0));
        let page = query.page;
        let filter = GrantFilter {
            role: role.0,
            resource: resource.0,
            action: action.0,
        };
        let listed = access.0.list(&caller, filter, query).await?;
        Ok(Json(ListResult::new(listed, page)))
    }

    /// Grant a role an action on a resource, within a scope (`any` unless given), taking effect
    /// on every server instance within 5 seconds. The resource, action and scope must be in the
    /// catalog (`get_access_catalog`), else `400` names the field at fault; a grant equal to an
    /// existing one is refused with `409`. `403` unless the caller is an `admin`, or holds a role granted it.
    #[oai(path = "/grants", method = "post", operation_id = "grant_access")]
    async fn grant(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        body: Json<GrantRequest>,
    ) -> ApiResult<WithETag<GrantResponse>> {
        let scope = body.0.scope();
        let GrantRequest {
            role,
            resource,
            action,
            ..
        } = body.0;
        let grant = access
            .0
            .grant(
                &caller,
                GrantAccess {
                    role,
                    resource,
                    action,
                    scope,
                },
            )
            .await?;
        Ok(tagged(grant))
    }

    /// Revoke a grant, taking effect on every server instance within 5 seconds. Requires
    /// `If-Match` with the version listed; a stale value is refused with `412`. A module's
    /// default grant, once revoked, isn't granted again by a later deploy. No revocation
    /// removes `admin`'s own access to these operations. `403` unless the caller is an `admin`, or holds a role granted it.
    #[oai(path = "/grants/:id", method = "delete", operation_id = "revoke_grant")]
    async fn revoke(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        id: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
    ) -> ApiResult<Revoked> {
        let expected = if_match_version(if_match.0)?;
        access.0.revoke(&caller, grant_id(&id.0)?, expected).await?;
        Ok(Revoked::Revoked)
    }

    /// What can be granted: every module's gate (`module:<name>`, action `use`) and resources,
    /// each action with the operations performing it and whether anything in force could allow
    /// it (`reachable`), and the scopes a grant of it may name. `403` unless the caller is an `admin`, or holds a role granted it.
    #[oai(path = "/catalog", method = "get", operation_id = "get_access_catalog")]
    async fn catalog(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
    ) -> ApiResult<Json<CatalogResponse>> {
        Ok(Json(access.0.catalog(&caller)?.into()))
    }
}
