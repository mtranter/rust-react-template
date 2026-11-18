/// A resource as admins grant it: a module's gate (`module:<name>`) or one of its resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogResource {
    pub module: String,
    pub resource: String,
    pub actions: Vec<CatalogAction>,
    /// The resource's own scopes, `any` aside.
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogAction {
    pub name: String,
    /// The operations performing it, by id.
    pub operations: Vec<&'static str>,
    /// Whether any rule or grant in force could allow it.
    pub reachable: bool,
}
