use std::collections::HashMap;
use std::fmt;

use crate::condition::Condition;
use crate::policy::{self, DerivedRole, DerivedRoleSet, Effect};
use crate::rule_set::{Roles, Rule};
use crate::yaml::File;
use crate::{Action, PolicyError, Resource, RuleSet};

/// A policy file's name, for refusals, and its YAML. `app_authz::policy_file!("policies/note.yaml")`
/// embeds one from the calling crate's root.
#[derive(Debug, Clone)]
pub struct PolicyFile {
    pub name: String,
    pub source: String,
}

impl PolicyFile {
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
        }
    }
}

#[macro_export]
macro_rules! policy_file {
    ($path:literal) => {
        $crate::PolicyFile::new(
            $path,
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path)),
        )
    };
}

/// Role → action, within a scope: what admins grant at runtime, and what a default grant expands to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Grant {
    pub role: String,
    pub resource: String,
    pub action: String,
    pub scope: Scope,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope {
    Any,
    /// A scope of the grant's resource: a derived role whose `parentRoles` is `["*"]`.
    Named(String),
}

impl fmt::Display for Grant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} → {}:{}, ", self.role, self.resource, self.action)?;
        match &self.scope {
            Scope::Any => f.write_str("any"),
            Scope::Named(s) => f.write_str(s),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("grant `{grant}` refused: {reason}")]
pub struct GrantRefused {
    pub grant: Box<Grant>,
    pub reason: GrantRefusal,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GrantRefusal {
    #[error("it names no role")]
    NoRole,
    #[error("`{0}` is not a resource of this module")]
    UnknownResource(String),
    #[error("`{action}` is not an action of `{resource}`")]
    UnknownAction { resource: String, action: String },
    #[error("`{scope}` is not a scope of `{resource}`")]
    UnknownScope { resource: String, scope: String },
}

/// One Module's policy files and default grants, checked against its resources' Rust types.
#[derive(Debug, Clone)]
pub struct Policies {
    module: String,
    label: Option<String>,
    resources: Vec<ResourceEntry>,
    default_grants: Vec<Grant>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResourceEntry {
    pub kind: String,
    pub actions: Vec<&'static str>,
    pub scopes: Vec<(String, Option<Condition>)>,
    pub rules: Vec<Rule>,
    pub label: Option<String>,
    pub action_labels: Vec<(String, String)>,
    pub scope_labels: Vec<(String, String)>,
}

impl ResourceEntry {
    fn new(kind: String, actions: Vec<&'static str>) -> Self {
        Self {
            kind,
            actions,
            scopes: Vec::new(),
            rules: Vec::new(),
            label: None,
            action_labels: Vec::new(),
            scope_labels: Vec::new(),
        }
    }

    fn scope(&self, name: &str) -> Option<&Option<Condition>> {
        self.scopes.iter().find(|(s, _)| s == name).map(|(_, c)| c)
    }
}

struct Given {
    kind: &'static str,
    actions: Vec<&'static str>,
    attrs: &'static [&'static str],
    file: PolicyFile,
}

pub struct PoliciesBuilder {
    module: String,
    resources: Vec<Given>,
    derived_roles: Vec<PolicyFile>,
    default_grants: Vec<PolicyFile>,
    labels: Vec<PolicyFile>,
}

impl Policies {
    pub fn builder(module: impl Into<String>) -> PoliciesBuilder {
        PoliciesBuilder {
            module: module.into(),
            resources: Vec::new(),
            derived_roles: Vec::new(),
            default_grants: Vec::new(),
            labels: Vec::new(),
        }
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    pub fn default_grants(&self) -> &[Grant] {
        &self.default_grants
    }

    /// Whether `kind` is one of this Module's resources.
    pub fn has_resource(&self, kind: &str) -> bool {
        self.resource(kind).is_some()
    }

    pub(crate) fn resources(&self) -> &[ResourceEntry] {
        &self.resources
    }

    pub(crate) fn resource(&self, kind: &str) -> Option<&ResourceEntry> {
        self.resources.iter().find(|r| r.kind == kind)
    }

    /// Whether `grant` could be saved: a resource of this Module, one of its actions, and `Any` or
    /// one of its scopes. Roles come from the identity provider, so their names aren't checked.
    pub fn check(&self, grant: &Grant) -> Result<(), GrantRefused> {
        self.grant_rule(grant).map(|_| ())
    }

    pub(crate) fn grant_rule(&self, grant: &Grant) -> Result<Rule, GrantRefused> {
        let refuse = |reason| GrantRefused {
            grant: Box::new(grant.clone()),
            reason,
        };
        if grant.role.is_empty() {
            return Err(refuse(GrantRefusal::NoRole));
        }
        let resource = self
            .resource(&grant.resource)
            .ok_or_else(|| refuse(GrantRefusal::UnknownResource(grant.resource.clone())))?;
        let action = resource
            .actions
            .iter()
            .find(|a| **a == grant.action)
            .ok_or_else(|| {
                refuse(GrantRefusal::UnknownAction {
                    resource: grant.resource.clone(),
                    action: grant.action.clone(),
                })
            })?;
        let condition = match &grant.scope {
            Scope::Any => None,
            Scope::Named(scope) => resource
                .scope(scope)
                .ok_or_else(|| {
                    refuse(GrantRefusal::UnknownScope {
                        resource: grant.resource.clone(),
                        scope: scope.clone(),
                    })
                })?
                .clone(),
        };
        Ok(Rule {
            actions: vec![action],
            roles: Roles::Named(vec![grant.role.clone()]),
            effect: Effect::Allow,
            condition,
        })
    }

    /// What admins grant from: each resource, their actions and scopes (`any` first),
    /// each with its label, or one made from its name. An action no rule in `rule_set` could allow
    /// is marked unreachable.
    pub fn catalog(&self, rule_set: &RuleSet) -> Catalog {
        let module_label = self.label.clone().unwrap_or_else(|| humanize(&self.module));
        Catalog {
            module: self.module.clone(),
            resources: self
                .resources
                .iter()
                .map(|r| {
                    let label_of = |labels: &[(String, String)], name: &str| {
                        labels
                            .iter()
                            .find(|(n, _)| n == name)
                            .map(|(_, l)| l.clone())
                    };
                    CatalogResource {
                        kind: r.kind.clone(),
                        label: r.label.clone().unwrap_or_else(|| humanize(&r.kind)),
                        actions: r
                            .actions
                            .iter()
                            .map(|a| CatalogAction {
                                name: (*a).to_owned(),
                                label: label_of(&r.action_labels, a).unwrap_or_else(|| humanize(a)),
                                reachable: rule_set.could_allow(&r.kind, a),
                            })
                            .collect(),
                        scopes: std::iter::once(ANY)
                            .chain(r.scopes.iter().map(|(s, _)| s.as_str()))
                            .map(|s| CatalogScope {
                                name: s.to_owned(),
                                label: label_of(&r.scope_labels, s).unwrap_or_else(|| humanize(s)),
                            })
                            .collect(),
                    }
                })
                .collect(),
            label: module_label,
        }
    }
}

const ANY: &str = "any";

/// A name as words: `items:add` → `Items add`, `note_summary` → `Note summary`.
fn humanize(name: &str) -> String {
    let words = name
        .split([':', '_', '-'])
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Whether `grant` could be saved, against whichever Module's policies have its resource.
pub fn check_grant(policies: &[Policies], grant: &Grant) -> Result<(), GrantRefused> {
    match policies
        .iter()
        .find(|p| p.resource(&grant.resource).is_some())
    {
        Some(p) => p.check(grant),
        None => Err(GrantRefused {
            grant: Box::new(grant.clone()),
            reason: GrantRefusal::UnknownResource(grant.resource.clone()),
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub module: String,
    /// What the Module calls itself to admins.
    pub label: String,
    pub resources: Vec<CatalogResource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogResource {
    pub kind: String,
    pub label: String,
    pub actions: Vec<CatalogAction>,
    /// `any` first, then the resource's own.
    pub scopes: Vec<CatalogScope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogAction {
    pub name: String,
    pub label: String,
    pub reachable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogScope {
    pub name: String,
    pub label: String,
}

/// `*` matches every action; otherwise each `:`-separated segment matches itself or `*`.
pub(crate) fn action_matches(pattern: &str, action: &str) -> bool {
    pattern == "*" || {
        let (p, a): (Vec<_>, Vec<_>) = (pattern.split(':').collect(), action.split(':').collect());
        p.len() == a.len() && p.iter().zip(&a).all(|(p, a)| *p == "*" || p == a)
    }
}

impl PoliciesBuilder {
    /// The Module whose policies these are, to name it when they don't load.
    pub fn module(&self) -> &str {
        &self.module
    }

    /// The policy for `R`; its `resource` must be `R::KIND`.
    pub fn resource<R: Resource>(mut self, file: PolicyFile) -> Self {
        self.resources.push(Given {
            kind: R::KIND,
            actions: R::Action::all().iter().map(|a| a.name()).collect(),
            attrs: R::ATTRS,
            file,
        });
        self
    }

    pub fn derived_roles(mut self, file: PolicyFile) -> Self {
        self.derived_roles.push(file);
        self
    }

    pub fn default_grants(mut self, file: PolicyFile) -> Self {
        self.default_grants.push(file);
        self
    }

    /// What admins see the Module, its resources, actions and scopes called; any left out is
    /// shown by its name, as words.
    pub fn labels(mut self, file: PolicyFile) -> Self {
        self.labels.push(file);
        self
    }

    pub fn build(self) -> Result<Policies, PolicyError> {
        let mut sets: HashMap<String, (String, DerivedRoleSet)> = HashMap::new();
        for pf in &self.derived_roles {
            let file = File { name: &pf.name };
            let set = policy::derived_roles(&file, &pf.source)?;
            let (name, line) = set.name.clone();
            if sets.contains_key(&name) {
                return Err(file.refuse(line, format!("derived roles `{name}` are already loaded")));
            }
            let mut seen = Vec::new();
            for d in &set.definitions {
                if seen.contains(&&d.name.0) {
                    return Err(file.refuse(
                        d.name.1,
                        format!("derived role `{}` is defined twice", d.name.0),
                    ));
                }
                if d.is_scope() && d.name.0 == "any" {
                    return Err(
                        file.refuse(d.name.1, "`any` names every row, so it can't name a scope")
                    );
                }
                seen.push(&d.name.0);
            }
            sets.insert(name, (pf.name.clone(), set));
        }

        let mut resources = Vec::new();
        for given in &self.resources {
            if resources
                .iter()
                .any(|r: &ResourceEntry| r.kind == given.kind)
            {
                let file = File {
                    name: &given.file.name,
                };
                return Err(file.refuse(1, format!("`{}` is given twice", given.kind)));
            }
            resources.push(self.resource_entry(given, &sets)?);
        }

        let mut default_grants = Vec::new();
        for pf in &self.default_grants {
            let file = File { name: &pf.name };
            for d in policy::default_grants(&file, &pf.source)? {
                if d.role.is_empty() {
                    return Err(file.refuse(d.line, "a default grant names a role"));
                }
                let (kind, line) = &d.resource;
                let Some(resource) = resources.iter().find(|r| &r.kind == kind) else {
                    return Err(file.refuse(*line, not_ours(kind, &self.module)));
                };
                let scope = match &d.scope {
                    None => Scope::Any,
                    Some((s, _)) if s == "any" => Scope::Any,
                    Some((s, line)) => {
                        if resource.scope(s).is_none() {
                            return Err(file.refuse(*line, self.why_not_a_scope(kind, s, &sets)));
                        }
                        Scope::Named(s.clone())
                    }
                };
                for (action, line) in &d.actions {
                    let actions: Vec<&str> = if action == "*" {
                        resource.actions.clone()
                    } else if resource.actions.contains(&action.as_str()) {
                        vec![action]
                    } else {
                        return Err(
                            file.refuse(*line, format!("`{action}` is not an action of `{kind}`"))
                        );
                    };
                    default_grants.extend(actions.into_iter().map(|a| Grant {
                        role: d.role.clone(),
                        resource: kind.clone(),
                        action: a.to_owned(),
                        scope: scope.clone(),
                    }));
                }
            }
        }

        let label = self.apply_labels(&mut resources, &sets)?;

        Ok(Policies {
            module: self.module,
            label,
            resources,
            default_grants,
        })
    }

    fn apply_labels(
        &self,
        resources: &mut [ResourceEntry],
        sets: &HashMap<String, (String, DerivedRoleSet)>,
    ) -> Result<Option<String>, PolicyError> {
        let mut module_label = None;
        let mut labelled: Vec<String> = Vec::new();
        for pf in &self.labels {
            let file = File { name: &pf.name };
            let labels = policy::labels(&file, &pf.source)?;
            if let Some(label) = labels.module {
                if module_label.is_some() {
                    return Err(
                        file.refuse(1, format!("the `{}` module is labelled twice", self.module))
                    );
                }
                module_label = Some(label);
            }
            for given in labels.resources {
                let (kind, line) = &given.resource;
                let Some(resource) = resources.iter_mut().find(|r| &r.kind == kind) else {
                    return Err(file.refuse(*line, not_ours(kind, &self.module)));
                };
                if labelled.contains(kind) {
                    return Err(file.refuse(*line, format!("`{kind}` is labelled twice")));
                }
                labelled.push(kind.clone());
                for ((action, line), _) in &given.actions {
                    if !resource.actions.contains(&action.as_str()) {
                        return Err(
                            file.refuse(*line, format!("`{action}` is not an action of `{kind}`"))
                        );
                    }
                }
                for ((scope, line), _) in &given.scopes {
                    if scope != ANY && resource.scope(scope).is_none() {
                        return Err(file.refuse(*line, self.why_not_a_scope(kind, scope, sets)));
                    }
                }
                resource.label = given.label;
                resource.action_labels = given
                    .actions
                    .into_iter()
                    .map(|((a, _), l)| (a, l))
                    .collect();
                resource.scope_labels =
                    given.scopes.into_iter().map(|((s, _), l)| (s, l)).collect();
            }
        }
        Ok(module_label)
    }

    fn resource_entry(
        &self,
        given: &Given,
        sets: &HashMap<String, (String, DerivedRoleSet)>,
    ) -> Result<ResourceEntry, PolicyError> {
        let file = File {
            name: &given.file.name,
        };
        let policy = policy::resource_policy(&file, &given.file.source)?;
        let (kind, line) = &policy.resource;
        if given.kind.starts_with(GATE_PREFIX) {
            return Err(file.refuse(*line, not_ours(given.kind, &self.module)));
        }
        if kind != given.kind {
            let reason = if self.resources.iter().any(|r| r.kind == kind) {
                format!(
                    "this is `{kind}`'s policy, but was given for `{}`",
                    given.kind
                )
            } else {
                not_ours(kind, &self.module)
            };
            return Err(file.refuse(*line, reason));
        }

        let mut imported: Vec<(&str, &DerivedRole)> = Vec::new();
        for (name, line) in &policy.imports {
            let Some((set_file, set)) = sets.get(name) else {
                return Err(file.refuse(*line, format!("derived roles `{name}` are not loaded")));
            };
            for d in &set.definitions {
                if imported.iter().any(|(_, i)| i.name.0 == d.name.0) {
                    return Err(file.refuse(
                        *line,
                        format!("derived role `{}` is imported twice", d.name.0),
                    ));
                }
                if let Some(c) = &d.condition {
                    c.check_attrs(set_file, given.kind, given.attrs)?;
                }
                imported.push((set_file, d));
            }
        }
        let derived = |name: &str| {
            imported
                .iter()
                .find(|(_, d)| d.name.0 == name)
                .map(|(_, d)| *d)
        };

        let mut rules = Vec::new();
        for rule in &policy.rules {
            let mut actions = Vec::new();
            for (pattern, line) in &rule.actions {
                let matched: Vec<_> = given
                    .actions
                    .iter()
                    .copied()
                    .filter(|a| action_matches(pattern, a))
                    .collect();
                if matched.is_empty() {
                    return Err(
                        file.refuse(*line, format!("`{pattern}` is not an action of `{kind}`"))
                    );
                }
                actions.extend(matched);
            }
            if let Some(c) = &rule.condition {
                c.check_attrs(&given.file.name, kind, given.attrs)?;
            }
            let condition = rule.condition.as_ref().map(|c| c.condition.clone());
            if !rule.roles.is_empty() {
                rules.push(Rule {
                    actions: actions.clone(),
                    roles: Roles::from(
                        rule.roles
                            .iter()
                            .map(|(r, _)| r.clone())
                            .collect::<Vec<_>>(),
                    ),
                    effect: rule.effect,
                    condition: condition.clone(),
                });
            }
            for (name, line) in &rule.derived_roles {
                let Some(d) = derived(name) else {
                    return Err(file.refuse(
                        *line,
                        format!("`{name}` is not a derived role imported by `{kind}`'s policy"),
                    ));
                };
                rules.push(Rule {
                    actions: actions.clone(),
                    roles: Roles::from(d.parent_roles.clone()),
                    effect: rule.effect,
                    condition: both(
                        d.condition.as_ref().map(|c| &c.condition),
                        condition.as_ref(),
                    ),
                });
            }
        }

        Ok(ResourceEntry {
            kind: kind.clone(),
            actions: given.actions.clone(),
            scopes: imported
                .iter()
                .filter(|(_, d)| d.is_scope())
                .map(|(_, d)| {
                    (
                        d.name.0.clone(),
                        d.condition.as_ref().map(|c| c.condition.clone()),
                    )
                })
                .collect(),
            rules,
            ..ResourceEntry::new(String::new(), Vec::new())
        })
    }

    fn why_not_a_scope(
        &self,
        kind: &str,
        scope: &str,
        sets: &HashMap<String, (String, DerivedRoleSet)>,
    ) -> String {
        let is_derived_role = sets
            .values()
            .flat_map(|(_, s)| &s.definitions)
            .any(|d| d.name.0 == scope);
        if is_derived_role {
            format!(
                "derived role `{scope}` can't be a scope of `{kind}`: a scope is imported by its policy and has `parentRoles: [\"*\"]`"
            )
        } else {
            format!("`{scope}` is not a scope of `{kind}`")
        }
    }
}

fn both(a: Option<&Condition>, b: Option<&Condition>) -> Option<Condition> {
    match (a, b) {
        (Some(a), Some(b)) => Some(Condition::All(vec![a.clone(), b.clone()])),
        (a, b) => a.or(b).cloned(),
    }
}

/// What a Module's gate was named before ADR 0047 removed gates: no resource may be named so.
const GATE_PREFIX: &str = "module:";

fn not_ours(kind: &str, module: &str) -> String {
    if kind.starts_with(GATE_PREFIX) {
        return format!(
            "`{kind}` names a Module's gate, and Modules have none (ADR 0047): grant the actions of its resources instead"
        );
    }
    format!(
        "`{kind}` is not a resource given to the `{module}` module's policies: a Module's policies name only its own resources"
    )
}
