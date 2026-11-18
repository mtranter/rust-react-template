//! Ids, timestamps, and the version are plain strings so the generated TS client stays
//! dependency-free.

use crate::application::{
    CatalogAction, CatalogResource, GrantSortKey, Listing, RoleSortKey, Versioned, scope_name,
};
use crate::domain::{Grant, Role, Scope};
use app_persistence::{Direction, Sort};
use app_web::SortParam;
use poem_openapi::{Enum, Object};

/// A role granted an action on a resource, within a scope. The role must exist (`list_roles`);
/// the resource, action and scope must be in the catalog (`get_access_catalog`).
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct GrantRequest {
    pub role: String,
    /// A module's gate (`module:<name>`, whose only action is `use`) or one of its resources.
    pub resource: String,
    pub action: String,
    /// `any` (every row), or one of the resource's scopes. A gate takes only `any`.
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

/// What admins grant from: every module's gate and resources.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct CatalogResponse {
    pub resources: Vec<CatalogResourceResponse>,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct CatalogResourceResponse {
    /// The module whose resource it is.
    pub module: String,
    /// `module:<name>` for a module's gate.
    pub resource: String,
    pub actions: Vec<CatalogActionResponse>,
    /// What a grant of it may be scoped to: `any` first, then the resource's own.
    pub scopes: Vec<String>,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct CatalogActionResponse {
    pub name: String,
    /// The operations performing it, by operation id.
    pub operations: Vec<String>,
    /// False when no rule or grant in force could allow it to anyone.
    pub reachable: bool,
}

impl From<Vec<CatalogResource>> for CatalogResponse {
    fn from(resources: Vec<CatalogResource>) -> Self {
        Self {
            resources: resources
                .into_iter()
                .map(|r| CatalogResourceResponse {
                    module: r.module,
                    resource: r.resource,
                    actions: r.actions.into_iter().map(Into::into).collect(),
                    scopes: std::iter::once("any".to_owned()).chain(r.scopes).collect(),
                })
                .collect(),
        }
    }
}

impl From<CatalogAction> for CatalogActionResponse {
    fn from(a: CatalogAction) -> Self {
        Self {
            name: a.name,
            operations: a.operations.into_iter().map(str::to_owned).collect(),
            reachable: a.reachable,
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
