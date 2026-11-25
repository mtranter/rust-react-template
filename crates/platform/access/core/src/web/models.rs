//! Ids, timestamps, and the version are plain strings so the generated TS client stays
//! dependency-free.

use crate::application::{
    CatalogAction, CatalogCapability, CatalogResource, DirectoryStatus, GrantSortKey, HeldRole,
    InvitationListing, InvitationSortKey, Listing, Permission, RoleSortKey, SubjectListing,
    SubjectSortKey, Versioned, scope_name,
};
use crate::domain::{Assignment, Grant, Invitation, InvitationStatus, Role, Scope};
use app_persistence::{Direction, Sort};
use app_web::SortParam;
use poem_openapi::{Enum, Object};

/// A role granted an action on a resource, within a scope. The role must exist (`list_roles`);
/// the resource, action and scope must be in the catalog (`get_access_catalog`).
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct GrantRequest {
    pub role: String,
    /// One of a module's resources.
    pub resource: String,
    pub action: String,
    /// `any` (every row), or one of the resource's scopes.
    #[oai(default = "any_scope")]
    pub scope: String,
}

fn any_scope() -> String {
    "any".into()
}

impl GrantRequest {
    pub fn scope(&self) -> Scope {
        match self.scope.trim() {
            "any" => Scope::Any,
            named => Scope::Named(named.to_owned()),
        }
    }
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct GrantResponse {
    pub id: String,
    pub role: String,
    pub resource: String,
    pub action: String,
    pub scope: String,
    /// Who granted it; absent for a module's default grant, applied when it was deployed.
    #[oai(skip_serializing_if_is_none)]
    pub granted_by: Option<String>,
    pub granted_at: String,
    /// Its module's policies no longer have its resource, action or scope (a later release
    /// removed it), so it allows nothing: revoke it.
    pub stale: bool,
    /// Echo it back in `If-Match` to revoke this grant.
    pub version: String,
}

impl GrantResponse {
    fn of(versioned: Versioned<Grant>, stale: bool) -> Self {
        let Versioned { entity: g, version } = versioned;
        Self {
            id: g.id().to_string(),
            role: g.role().to_owned(),
            resource: g.resource().to_owned(),
            action: g.action().to_owned(),
            scope: scope_name(g.scope()).to_owned(),
            granted_by: g.granted_by().subject().map(str::to_owned),
            granted_at: g.granted_at().to_rfc3339(),
            stale,
            version: version.as_i64().to_string(),
        }
    }
}

impl From<Listing> for GrantResponse {
    fn from(listing: Listing) -> Self {
        Self::of(listing.grant, listing.stale)
    }
}

impl From<Versioned<Grant>> for GrantResponse {
    /// A grant just made was checked against the policies, so it isn't stale.
    fn from(grant: Versioned<Grant>) -> Self {
        Self::of(grant, false)
    }
}

/// What admins grant from: every Capability, each with its Modules' resources.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct CatalogResponse {
    pub capabilities: Vec<CatalogCapabilityResponse>,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct CatalogCapabilityResponse {
    /// The Capability's id: the product of the suite whose Modules these resources are.
    pub capability: String,
    /// What the Capability is called, for people.
    pub label: String,
    pub resources: Vec<CatalogResourceResponse>,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct CatalogResourceResponse {
    /// The module whose resource it is.
    pub module: String,
    /// What the module calls itself, for people.
    pub module_label: String,
    pub resource: String,
    /// What the module calls the resource, for people.
    pub label: String,
    pub actions: Vec<CatalogActionResponse>,
    /// What a grant of it may be scoped to: `any` first, then the resource's own.
    pub scopes: Vec<CatalogScopeResponse>,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct CatalogScopeResponse {
    /// What a grant names.
    pub name: String,
    /// The rows it reaches, for people (`Notes they wrote`).
    pub label: String,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct CatalogActionResponse {
    pub name: String,
    /// What the module calls the action, for people.
    pub label: String,
    /// The operations performing it, by operation id.
    pub operations: Vec<String>,
    /// False when no rule or grant in force could allow it to anyone.
    pub reachable: bool,
}

impl From<Vec<CatalogCapability>> for CatalogResponse {
    fn from(capabilities: Vec<CatalogCapability>) -> Self {
        Self {
            capabilities: capabilities
                .into_iter()
                .map(|c| CatalogCapabilityResponse {
                    capability: c.capability,
                    label: c.label,
                    resources: c.resources.into_iter().map(Into::into).collect(),
                })
                .collect(),
        }
    }
}

impl From<CatalogResource> for CatalogResourceResponse {
    fn from(r: CatalogResource) -> Self {
        CatalogResourceResponse {
            module: r.module,
            module_label: r.module_label,
            resource: r.resource,
            label: r.label,
            actions: r.actions.into_iter().map(Into::into).collect(),
            scopes: r
                .scopes
                .into_iter()
                .map(|s| CatalogScopeResponse {
                    name: s.name,
                    label: s.label,
                })
                .collect(),
        }
    }
}

impl From<CatalogAction> for CatalogActionResponse {
    fn from(a: CatalogAction) -> Self {
        Self {
            name: a.name,
            label: a.label,
            operations: a.operations.into_iter().map(str::to_owned).collect(),
            reachable: a.reachable,
        }
    }
}

/// What the caller may do: each action they may perform on some row of its resource.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct PermissionsResponse {
    pub permissions: Vec<PermissionResponse>,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct PermissionResponse {
    pub resource: String,
    pub action: String,
}

impl From<Vec<Permission>> for PermissionsResponse {
    fn from(permissions: Vec<Permission>) -> Self {
        Self {
            permissions: permissions
                .into_iter()
                .map(|p| PermissionResponse {
                    resource: p.resource,
                    action: p.action,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, Enum)]
pub enum GrantSort {
    #[oai(rename = "role")]
    RoleAsc,
    #[oai(rename = "-role")]
    RoleDesc,
    #[oai(rename = "resource")]
    ResourceAsc,
    #[oai(rename = "-resource")]
    ResourceDesc,
    #[oai(rename = "action")]
    ActionAsc,
    #[oai(rename = "-action")]
    ActionDesc,
    #[oai(rename = "grantedAt")]
    GrantedAtAsc,
    #[oai(rename = "-grantedAt")]
    GrantedAtDesc,
}

impl SortParam for GrantSort {
    type Key = GrantSortKey;

    fn sort(self) -> Sort<GrantSortKey> {
        let (key, direction) = match self {
            Self::RoleAsc => (GrantSortKey::Role, Direction::Asc),
            Self::RoleDesc => (GrantSortKey::Role, Direction::Desc),
            Self::ResourceAsc => (GrantSortKey::Resource, Direction::Asc),
            Self::ResourceDesc => (GrantSortKey::Resource, Direction::Desc),
            Self::ActionAsc => (GrantSortKey::Action, Direction::Asc),
            Self::ActionDesc => (GrantSortKey::Action, Direction::Desc),
            Self::GrantedAtAsc => (GrantSortKey::GrantedAt, Direction::Asc),
            Self::GrantedAtDesc => (GrantSortKey::GrantedAt, Direction::Desc),
        };
        Sort { key, direction }
    }
}

/// A role to create: its name is its id, which grants name and which can't be changed.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct RoleRequest {
    pub name: String,
    /// What the role is for, for the admins granting it.
    #[oai(default)]
    pub description: String,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct RoleResponse {
    pub name: String,
    pub description: String,
    /// `admin` and `user`, which can't be deleted.
    pub built_in: bool,
    /// Who created it; absent for a built-in role, or one created for a module's default grants.
    #[oai(skip_serializing_if_is_none)]
    pub created_by: Option<String>,
    pub created_at: String,
    /// Echo it back in `If-Match` to delete this role.
    pub version: String,
}

impl From<Versioned<Role>> for RoleResponse {
    fn from(versioned: Versioned<Role>) -> Self {
        let Versioned { entity: r, version } = versioned;
        Self {
            name: r.name().to_owned(),
            description: r.description().to_owned(),
            built_in: r.is_built_in(),
            created_by: r.created_by().subject().map(str::to_owned),
            created_at: r.created_at().to_rfc3339(),
            version: version.as_i64().to_string(),
        }
    }
}

/// A role to assign to a subject.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct AssignmentRequest {
    /// Whom to assign it to: the token's `sub`, whether or not they have signed in yet.
    pub subject: String,
    /// A role that exists (`list_roles`), never `user`, which every signed-in user holds.
    pub role: String,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct AssignmentResponse {
    pub subject: String,
    pub role: String,
    /// Who assigned it; absent for one a seed assigned.
    #[oai(skip_serializing_if_is_none)]
    pub assigned_by: Option<String>,
    pub assigned_at: String,
    /// Echo it back in `If-Match` to unassign the role.
    pub version: String,
}

impl From<Versioned<Assignment>> for AssignmentResponse {
    fn from(versioned: Versioned<Assignment>) -> Self {
        let Versioned { entity: a, version } = versioned;
        Self {
            subject: a.subject().to_owned(),
            role: a.role().to_owned(),
            assigned_by: a.assigned_by().subject().map(str::to_owned),
            assigned_at: a.assigned_at().to_rfc3339(),
            version: version.as_i64().to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Enum)]
pub enum RoleSort {
    #[oai(rename = "name")]
    NameAsc,
    #[oai(rename = "-name")]
    NameDesc,
    #[oai(rename = "createdAt")]
    CreatedAtAsc,
    #[oai(rename = "-createdAt")]
    CreatedAtDesc,
}

impl SortParam for RoleSort {
    type Key = RoleSortKey;

    fn sort(self) -> Sort<RoleSortKey> {
        let (key, direction) = match self {
            Self::NameAsc => (RoleSortKey::Name, Direction::Asc),
            Self::NameDesc => (RoleSortKey::Name, Direction::Desc),
            Self::CreatedAtAsc => (RoleSortKey::CreatedAt, Direction::Asc),
            Self::CreatedAtDesc => (RoleSortKey::CreatedAt, Direction::Desc),
        };
        Sort { key, direction }
    }
}

/// Someone roles can be assigned to: a subject seen signing in, or one assigned a role or made a
/// bootstrap admin before it ever has, which has no name, email or sightings.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct SubjectResponse {
    /// The token's `sub`: what `assign_role` and `unassign_role` take.
    pub subject: String,
    /// From the token's `name` claim, when it carried one, else from the invitation.
    #[oai(skip_serializing_if_is_none)]
    pub name: Option<String>,
    /// From the token's `email` claim, when it carried one, else from the invitation.
    #[oai(skip_serializing_if_is_none)]
    pub email: Option<String>,
    /// When it first signed in; absent if it never has.
    #[oai(skip_serializing_if_is_none)]
    pub first_seen: Option<String>,
    /// When it last signed in, accurate to within an hour; absent if it never has.
    #[oai(skip_serializing_if_is_none)]
    pub last_seen: Option<String>,
    /// In `APP_BOOTSTRAP_ADMINS`: it holds `admin` whatever is assigned, and it can't be
    /// unassigned.
    pub bootstrap_admin: bool,
    /// The roles it holds beside `user`, which every signed-in user holds, by name.
    pub roles: Vec<SubjectRoleResponse>,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct SubjectRoleResponse {
    pub role: String,
    /// The assignment's version: echo it back in `If-Match` to unassign the role. Absent for a
    /// bootstrap admin's `admin` that isn't also assigned, which can't be unassigned.
    #[oai(skip_serializing_if_is_none)]
    pub version: Option<String>,
}

impl From<HeldRole> for SubjectRoleResponse {
    fn from(held: HeldRole) -> Self {
        Self {
            role: held.role,
            version: held.version.map(|v| v.as_i64().to_string()),
        }
    }
}

impl From<SubjectListing> for SubjectResponse {
    fn from(listing: SubjectListing) -> Self {
        let s = listing.subject;
        Self {
            subject: s.subject,
            name: s.name,
            email: s.email,
            first_seen: s.first_seen.map(|at| at.to_rfc3339()),
            last_seen: s.last_seen.map(|at| at.to_rfc3339()),
            bootstrap_admin: listing.bootstrap_admin,
            roles: listing.roles.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, Enum)]
pub enum SubjectSort {
    #[oai(rename = "subject")]
    SubjectAsc,
    #[oai(rename = "-subject")]
    SubjectDesc,
    #[oai(rename = "name")]
    NameAsc,
    #[oai(rename = "-name")]
    NameDesc,
    #[oai(rename = "email")]
    EmailAsc,
    #[oai(rename = "-email")]
    EmailDesc,
    #[oai(rename = "lastSeen")]
    LastSeenAsc,
    #[oai(rename = "-lastSeen")]
    LastSeenDesc,
}

impl SortParam for SubjectSort {
    type Key = SubjectSortKey;

    fn sort(self) -> Sort<SubjectSortKey> {
        let (key, direction) = match self {
            Self::SubjectAsc => (SubjectSortKey::Subject, Direction::Asc),
            Self::SubjectDesc => (SubjectSortKey::Subject, Direction::Desc),
            Self::NameAsc => (SubjectSortKey::Name, Direction::Asc),
            Self::NameDesc => (SubjectSortKey::Name, Direction::Desc),
            Self::EmailAsc => (SubjectSortKey::Email, Direction::Asc),
            Self::EmailDesc => (SubjectSortKey::Email, Direction::Desc),
            Self::LastSeenAsc => (SubjectSortKey::LastSeen, Direction::Asc),
            Self::LastSeenDesc => (SubjectSortKey::LastSeen, Direction::Desc),
        };
        Sort { key, direction }
    }
}

/// Someone to invite: the identity provider creates them and mails them a temporary password.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct InvitationRequest {
    /// Where the invitation is sent, and what they sign in with.
    pub email: String,
    /// Shown to admins until their token carries one.
    #[oai(default)]
    pub name: String,
    /// Roles they hold beside `user` from now on: each must exist (`list_roles`), never `user`.
    #[oai(default)]
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
#[oai(rename_all = "lowercase")]
pub enum InvitationStatusParam {
    /// The invitee hasn't signed in yet.
    Pending,
    /// The invitee has signed in.
    Accepted,
    Revoked,
}

impl From<InvitationStatusParam> for InvitationStatus {
    fn from(status: InvitationStatusParam) -> Self {
        match status {
            InvitationStatusParam::Pending => Self::Pending,
            InvitationStatusParam::Accepted => Self::Accepted,
            InvitationStatusParam::Revoked => Self::Revoked,
        }
    }
}

impl From<InvitationStatus> for InvitationStatusParam {
    fn from(status: InvitationStatus) -> Self {
        match status {
            InvitationStatus::Pending => Self::Pending,
            InvitationStatus::Accepted => Self::Accepted,
            InvitationStatus::Revoked => Self::Revoked,
        }
    }
}

/// Where a pending invitee is in the identity provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
#[oai(rename_all = "camelCase")]
pub enum DirectoryStatusResponse {
    /// Waiting for them to sign in with the temporary password mailed to them.
    Waiting,
    /// The temporary password lapsed: resend the invitation.
    Expired,
    /// They have set a password but not reached the app yet.
    PasswordSet,
    /// Deleted from the identity provider outside the app: revoke the invitation.
    Missing,
}

impl From<DirectoryStatus> for DirectoryStatusResponse {
    fn from(status: DirectoryStatus) -> Self {
        match status {
            DirectoryStatus::Pending => Self::Waiting,
            DirectoryStatus::Expired => Self::Expired,
            DirectoryStatus::Active => Self::PasswordSet,
            DirectoryStatus::Missing => Self::Missing,
        }
    }
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct InvitationResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    /// The identity provider's `sub` for the invitee: the subject the roles are assigned to.
    pub subject: String,
    pub roles: Vec<String>,
    pub status: InvitationStatusParam,
    /// For a pending invitation, where the invitee is in the identity provider; absent
    /// otherwise, or when it couldn't say.
    #[oai(skip_serializing_if_is_none)]
    pub directory_status: Option<DirectoryStatusResponse>,
    /// Who invited them; absent for one made as `System`.
    #[oai(skip_serializing_if_is_none)]
    pub invited_by: Option<String>,
    pub invited_at: String,
    /// Echo it back in `If-Match` to resend or revoke this invitation.
    pub version: String,
}

impl InvitationResponse {
    fn of(versioned: Versioned<Invitation>, directory: Option<DirectoryStatus>) -> Self {
        let Versioned { entity: i, version } = versioned;
        Self {
            id: i.id().to_string(),
            email: i.email().as_str().to_owned(),
            name: i.name().to_owned(),
            subject: i.subject().to_owned(),
            roles: i.roles().to_vec(),
            status: i.status().into(),
            directory_status: directory.map(Into::into),
            invited_by: i.invited_by().subject().map(str::to_owned),
            invited_at: i.invited_at().to_rfc3339(),
            version: version.as_i64().to_string(),
        }
    }
}

impl From<InvitationListing> for InvitationResponse {
    fn from(listing: InvitationListing) -> Self {
        Self::of(listing.invitation, listing.directory)
    }
}

impl From<Versioned<Invitation>> for InvitationResponse {
    fn from(invitation: Versioned<Invitation>) -> Self {
        Self::of(invitation, None)
    }
}

#[derive(Debug, Clone, Copy, Enum)]
pub enum InvitationSort {
    #[oai(rename = "email")]
    EmailAsc,
    #[oai(rename = "-email")]
    EmailDesc,
    #[oai(rename = "invitedAt")]
    InvitedAtAsc,
    #[oai(rename = "-invitedAt")]
    InvitedAtDesc,
}

impl SortParam for InvitationSort {
    type Key = InvitationSortKey;

    fn sort(self) -> Sort<InvitationSortKey> {
        let (key, direction) = match self {
            Self::EmailAsc => (InvitationSortKey::Email, Direction::Asc),
            Self::EmailDesc => (InvitationSortKey::Email, Direction::Desc),
            Self::InvitedAtAsc => (InvitationSortKey::InvitedAt, Direction::Asc),
            Self::InvitedAtDesc => (InvitationSortKey::InvitedAt, Direction::Desc),
        };
        Sort { key, direction }
    }
}
