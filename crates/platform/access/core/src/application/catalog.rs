use app_authz::CatalogScope;

/// A module's resource, as admins grant it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogResource {
    pub module: String,
    pub module_label: String,
    pub resource: String,
    pub label: String,
    pub actions: Vec<CatalogAction>,
    /// `any` first, then the resource's own.
    pub scopes: Vec<CatalogScope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogAction {
    pub name: String,
    pub label: String,
    /// The operations performing it, by id.
    pub operations: Vec<&'static str>,
    /// Whether any rule or grant in force could allow it.
    pub reachable: bool,
}
