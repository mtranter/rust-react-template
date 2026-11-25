use super::models::{
    AssignmentRequest, AssignmentResponse, CatalogResponse, GrantRequest, GrantResponse, GrantSort,
    InvitationRequest, InvitationResponse, InvitationSort, InvitationStatusParam,
    PermissionsResponse, RoleRequest, RoleResponse, RoleSort, SubjectResponse, SubjectSort,
};
use crate::application::{AccessService, GrantFilter, InvitationFilter, SubjectFilter, Versioned};
use crate::domain::{
    AssignRole, CreateRole, Grant, GrantAccess, GrantId, InvitationId, InviteUser,
};
use app_error::AppError;
use app_web::{ApiResult, Caller, ListResult, WithETag, if_match_version, paged, sorts};
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

#[derive(ApiResponse)]
enum Deleted {
    #[oai(status = 204)]
    Deleted,
}

#[derive(ApiResponse)]
enum Unassigned {
    #[oai(status = 204)]
    Unassigned,
}

#[derive(ApiResponse)]
enum RevokedInvitation {
    #[oai(status = 204)]
    Revoked,
}

fn invitation_id(raw: &str) -> ApiResult<InvitationId> {
    raw.parse()
        .map_err(|_| AppError::validation("invalid invitation id").into())
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
    /// on every server instance within 5 seconds. The role must exist (`list_roles`), and the
    /// resource, action and scope must be in the catalog (`get_access_catalog`), else `400`
    /// names the field at fault; a grant equal to an
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

    /// What can be granted: every module's resources,
    /// each action with the operations performing it and whether anything in force could allow
    /// it (`reachable`), and the scopes a grant of it may name, each with a `label` to show people. `403` unless the caller is an `admin`, or holds a role granted it.
    #[oai(path = "/catalog", method = "get", operation_id = "get_access_catalog")]
    async fn catalog(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
    ) -> ApiResult<Json<CatalogResponse>> {
        Ok(Json(access.0.catalog(&caller)?.into()))
    }

    /// What the caller may do: every resource and action, by name, that the rules in force let
    /// the caller perform on at least one row, built-in rules included, sorted by resource then
    /// action. An action scoped to some rows (a note's author) is listed; one refused outright
    /// isn't. Any signed-in caller may ask, and it answers only for them. The server still
    /// refuses each operation the caller may not perform.
    #[oai(
        path = "/me/permissions",
        method = "get",
        operation_id = "get_my_permissions"
    )]
    async fn my_permissions(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
    ) -> ApiResult<Json<PermissionsResponse>> {
        Ok(Json(access.0.permissions(&caller).into()))
    }

    /// List the roles grants can be made to, each with its description and whether it is built
    /// in (`admin` and `user`, which can't be deleted). Paginated, ordered by name unless `sort`
    /// says otherwise (`name` or `createdAt`, `-` prefix for descending). `403` unless the caller
    /// is an `admin`, or holds a role granted it.
    #[oai(path = "/roles", method = "get", operation_id = "list_roles")]
    async fn list_roles(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
        sort: Query<Option<Vec<RoleSort>>>,
    ) -> ApiResult<Json<ListResult<RoleResponse>>> {
        let query = paged(limit.0, offset.0).sorted(sorts(sort.0));
        let page = query.page;
        let listed = access.0.list_roles(&caller, query).await?;
        Ok(Json(ListResult::new(listed, page)))
    }

    /// Create a role, so grants can be made to it. Its name is its id and can't be changed
    /// later; a name a role already has is refused with `409`, and an empty one with `400`
    /// naming `name`. `403` unless the caller is an `admin`, or holds a role granted it.
    #[oai(path = "/roles", method = "post", operation_id = "create_role")]
    async fn create_role(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        body: Json<RoleRequest>,
    ) -> ApiResult<WithETag<RoleResponse>> {
        let RoleRequest { name, description } = body.0;
        let role = access
            .0
            .create_role(&caller, CreateRole { name, description })
            .await?;
        let version = role.version;
        Ok(WithETag::new(role.into(), version))
    }

    /// Delete a role. Requires `If-Match` with the version listed; a stale value is refused with
    /// `412`. A built-in role (`admin`, `user`), or one a grant still names, is refused with
    /// `409`: revoke its grants (`list_grants` filtered by `role`, then `revoke_grant`) first.
    /// `404` when no role has that name. `403` unless the caller is an `admin`, or holds a role
    /// granted it.
    #[oai(path = "/roles/:name", method = "delete", operation_id = "delete_role")]
    async fn delete_role(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        name: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
    ) -> ApiResult<Deleted> {
        let expected = if_match_version(if_match.0)?;
        access.0.delete_role(&caller, &name.0, expected).await?;
        Ok(Deleted::Deleted)
    }

    /// List the subjects roles can be assigned to (each a token's `sub`): everyone who has
    /// signed in, with their name and email when their token carried them and when they were
    /// last seen (to within an hour), and anyone assigned a role or in `APP_BOOTSTRAP_ADMINS`
    /// who hasn't signed in yet. Each comes with the roles it holds beside `user`, and each
    /// role with the version `unassign_role` takes; a bootstrap admin is marked `bootstrapAdmin`
    /// and holds `admin`, which can't be unassigned. `search` matches part of the subject, name
    /// or email, in any case. Paginated, ordered by subject unless `sort` says otherwise
    /// (`subject`, `name`, `email` or `lastSeen`, `-` prefix for descending). `403` unless the
    /// caller is an `admin`, or holds a role granted it.
    #[oai(path = "/subjects", method = "get", operation_id = "list_subjects")]
    async fn list_subjects(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        search: Query<Option<String>>,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
        sort: Query<Option<Vec<SubjectSort>>>,
    ) -> ApiResult<Json<ListResult<SubjectResponse>>> {
        let query = paged(limit.0, offset.0).sorted(sorts(sort.0));
        let page = query.page;
        let filter = SubjectFilter { search: search.0 };
        let listed = access.0.list_subjects(&caller, filter, query).await?;
        Ok(Json(ListResult::new(listed, page)))
    }

    /// Assign a role to a subject (the token's `sub`), whether or not they have signed in yet.
    /// They hold it, beside `user`, on every server instance within 5 seconds; the token's own
    /// claims never give roles. `user`, which every signed-in user holds, is refused with `400`
    /// naming `role`, as is a role that doesn't exist (`list_roles`, `create_role`); a role the
    /// subject holds already is refused with `409`. `403` unless the caller is an `admin`, or
    /// holds a role granted it.
    #[oai(path = "/assignments", method = "post", operation_id = "assign_role")]
    async fn assign_role(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        body: Json<AssignmentRequest>,
    ) -> ApiResult<WithETag<AssignmentResponse>> {
        let AssignmentRequest { subject, role } = body.0;
        let assignment = access
            .0
            .assign_role(&caller, AssignRole { subject, role })
            .await?;
        let version = assignment.version;
        Ok(WithETag::new(assignment.into(), version))
    }

    /// Unassign a role from a subject, taking effect on every server instance within 5 seconds.
    /// Requires `If-Match` with the version `assign_role` answered; a stale value is refused with
    /// `412`, and `404` when the subject doesn't hold that role by assignment. `user` is refused
    /// with `400` naming `role`. `admin` is refused with `409` for a bootstrap admin
    /// (`APP_BOOTSTRAP_ADMINS`), who always holds it, and for the last subject assigned it when
    /// there are no bootstrap admins, so admins can't lock themselves out. `403` unless the caller
    /// is an `admin`, or holds a role granted it.
    #[oai(
        path = "/assignments",
        method = "delete",
        operation_id = "unassign_role"
    )]
    async fn unassign_role(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        subject: Query<String>,
        role: Query<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
    ) -> ApiResult<Unassigned> {
        let expected = if_match_version(if_match.0)?;
        access
            .0
            .unassign_role(&caller, &subject.0, &role.0, expected)
            .await?;
        Ok(Unassigned::Unassigned)
    }

    /// Invite someone by email: the identity provider (Cognito) creates their account and
    /// mails them a temporary password, and they hold `roles` (each must exist: `list_roles`;
    /// never `user`) beside `user` on every server instance within 5 seconds, from before they
    /// first sign in. The answer's `subject` is their `sub`, as `list_subjects` lists them.
    /// Refused with `400` naming `email`, `name` or `roles` for one at fault; `409` when an
    /// invitation to that email is pending already (`resend_invitation` it) or the identity
    /// provider has a user with that email who has set a password (`assign_role` to them
    /// instead); `501` when no identity provider is configured; `503` when it can't be reached
    /// or is throttling: retry later. `403` unless the caller is an `admin`, or holds a role
    /// granted it.
    #[oai(path = "/invitations", method = "post", operation_id = "invite_user")]
    async fn invite_user(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        body: Json<InvitationRequest>,
    ) -> ApiResult<WithETag<InvitationResponse>> {
        let InvitationRequest { email, name, roles } = body.0;
        let invitation = access
            .0
            .invite(&caller, InviteUser { email, name, roles })
            .await?;
        let version = invitation.version;
        Ok(WithETag::new(invitation.into(), version))
    }

    /// List invitations, newest first unless `sort` says otherwise (`email` or `invitedAt`,
    /// `-` prefix for descending), filtered by `status` (`pending`: not signed in yet,
    /// `accepted`: signed in, `revoked`). Each pending one says where its invitee is in the
    /// identity provider (`directoryStatus`): `waiting`, `expired` (resend it), `passwordSet`,
    /// or `missing` (deleted there: revoke it). Paginated. `403` unless the caller is an
    /// `admin`, or holds a role granted it.
    #[oai(
        path = "/invitations",
        method = "get",
        operation_id = "list_invitations"
    )]
    async fn list_invitations(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        status: Query<Option<InvitationStatusParam>>,
        limit: Query<Option<i64>>,
        offset: Query<Option<i64>>,
        sort: Query<Option<Vec<InvitationSort>>>,
    ) -> ApiResult<Json<ListResult<InvitationResponse>>> {
        let query = paged(limit.0, offset.0).sorted(sorts(sort.0));
        let page = query.page;
        let filter = InvitationFilter {
            status: status.0.map(Into::into),
        };
        let listed = access.0.list_invitations(&caller, filter, query).await?;
        Ok(Json(ListResult::new(listed, page)))
    }

    /// Have the identity provider mail a pending invitee a fresh temporary password, as when
    /// theirs has expired. Requires `If-Match` with the version listed; a stale value is
    /// refused with `412`. `409` once the invitation is accepted or revoked, or when the
    /// invitee has set a password or was deleted from the identity provider; `404` when no
    /// invitation has that id; `501` when no identity provider is configured; `503` when it
    /// can't be reached. `403` unless the caller is an `admin`, or holds a role granted it.
    #[oai(
        path = "/invitations/:id/resend",
        method = "post",
        operation_id = "resend_invitation"
    )]
    async fn resend_invitation(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        id: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
    ) -> ApiResult<WithETag<InvitationResponse>> {
        let expected = if_match_version(if_match.0)?;
        let invitation = access
            .0
            .resend_invitation(&caller, invitation_id(&id.0)?, expected)
            .await?;
        let version = invitation.version;
        Ok(WithETag::new(invitation.into(), version))
    }

    /// Revoke a pending invitation: the identity provider deletes the invitee's account, and
    /// the roles it assigned are unassigned on every server instance within 5 seconds.
    /// Requires `If-Match` with the version listed; a stale value is refused with `412`. `409`
    /// once the invitation is accepted or revoked, or when the invitee has set a password
    /// (`unassign_role` instead); `404` when no invitation has that id; `501` when no identity
    /// provider is configured; `503` when it can't be reached. `403` unless the caller is an
    /// `admin`, or holds a role granted it.
    #[oai(
        path = "/invitations/:id",
        method = "delete",
        operation_id = "revoke_invitation"
    )]
    async fn revoke_invitation(
        &self,
        access: Data<&AccessService>,
        caller: Caller,
        id: Path<String>,
        #[oai(name = "If-Match")] if_match: Header<Option<String>>,
    ) -> ApiResult<RevokedInvitation> {
        let expected = if_match_version(if_match.0)?;
        access
            .0
            .revoke_invitation(&caller, invitation_id(&id.0)?, expected)
            .await?;
        Ok(RevokedInvitation::Revoked)
    }
}
