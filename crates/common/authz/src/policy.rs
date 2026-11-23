//! ADR 0040's three kinds of policy file, parsed but not yet checked against a resource.

use crate::PolicyError;
use crate::condition::{Located, parse_condition};
use crate::yaml::{Fields, File, Kind, Node};

pub(crate) const API_VERSION: &str = "auth.flagrante.io/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Effect {
    Allow,
    Deny,
}

/// A name and the line naming it.
pub(crate) type Named = (String, usize);

#[derive(Debug, Clone)]
pub(crate) struct ResourcePolicy {
    pub resource: Named,
    pub imports: Vec<Named>,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone)]
pub(crate) struct Rule {
    pub line: usize,
    pub actions: Vec<Named>,
    pub effect: Effect,
    pub roles: Vec<Named>,
    pub derived_roles: Vec<Named>,
    pub condition: Option<Located>,
}

#[derive(Debug, Clone)]
pub(crate) struct DerivedRoleSet {
    pub name: Named,
    pub definitions: Vec<DerivedRole>,
}

#[derive(Debug, Clone)]
pub(crate) struct DerivedRole {
    pub name: Named,
    pub parent_roles: Vec<String>,
    pub condition: Option<Located>,
}

impl DerivedRole {
    /// A derived role that means only its condition, so a grant can pair it with any role.
    pub fn is_scope(&self) -> bool {
        self.parent_roles == ["*"]
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DefaultGrant {
    pub line: usize,
    pub role: String,
    pub resource: Named,
    pub actions: Vec<Named>,
    pub scope: Option<Named>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Expect {
    ResourcePolicy,
    DerivedRoles,
    DefaultGrants,
    Labels,
}

impl Expect {
    fn key(self) -> &'static str {
        match self {
            Expect::ResourcePolicy => "resourcePolicy",
            Expect::DerivedRoles => "derivedRoles",
            Expect::DefaultGrants => "defaultGrants",
            Expect::Labels => "labels",
        }
    }
}

/// The document's one policy, after checking `apiVersion` and that it is the kind expected.
fn body<'n>(file: &File, root: &'n Node, expect: Expect) -> Result<&'n Node, PolicyError> {
    let mut fields = file.map(root, "a policy file")?;
    let version = fields.require(file, "apiVersion")?;
    if file.str(version, "`apiVersion`")? != API_VERSION {
        return Err(file.refuse(
            version.line,
            format!("`apiVersion` must be `{API_VERSION}`"),
        ));
    }
    let kinds = [
        Expect::ResourcePolicy,
        Expect::DerivedRoles,
        Expect::DefaultGrants,
        Expect::Labels,
    ];
    if !fields.has(expect.key())
        && let Some(other) = kinds.into_iter().find(|k| fields.has(k.key()))
    {
        return Err(file.refuse(
            fields.key_line(other.key()).unwrap_or(root.line),
            format!("expected a `{}`, found a `{}`", expect.key(), other.key()),
        ));
    }
    let Some(body) = fields.take(expect.key()) else {
        fields.finish(file, "a policy file")?;
        return Err(file.refuse(root.line, format!("expected a `{}`", expect.key())));
    };
    fields.finish(file, "a policy file")?;
    Ok(body)
}

pub(crate) fn resource_policy(file: &File, source: &str) -> Result<ResourcePolicy, PolicyError> {
    let root = file.parse(source)?;
    let node = body(file, &root, Expect::ResourcePolicy)?;
    let mut fields = file.map(node, "`resourcePolicy`")?;
    let resource = named(file, fields.require(file, "resource")?, "`resource`")?;
    let version = fields.require(file, "version")?;
    if file.str(version, "`version`")? != "default" {
        return Err(file.refuse(version.line, "`version` must be `default`"));
    }
    let imports = match fields.take("importDerivedRoles") {
        Some(n) => file.strings(n, "`importDerivedRoles`")?,
        None => Vec::new(),
    };
    let rules = file
        .seq(fields.require(file, "rules")?, "`rules`")?
        .iter()
        .map(|n| rule(file, n))
        .collect::<Result<_, _>>()?;
    fields.finish(file, "`resourcePolicy`")?;
    Ok(ResourcePolicy {
        resource,
        imports,
        rules,
    })
}

fn rule(file: &File, node: &Node) -> Result<Rule, PolicyError> {
    let mut fields = file.map(node, "a rule")?;
    if let Some(name) = fields.take("name") {
        file.str(name, "`name`")?;
    }
    let actions = non_empty(file, &mut fields, "actions")?;
    let effect = fields.require(file, "effect")?;
    let effect = match file.str(effect, "`effect`")? {
        "EFFECT_ALLOW" => Effect::Allow,
        "EFFECT_DENY" => Effect::Deny,
        _ => return Err(file.refuse(effect.line, "`effect` is `EFFECT_ALLOW` or `EFFECT_DENY`")),
    };
    let roles = optional_strings(file, &mut fields, "roles")?;
    let derived_roles = optional_strings(file, &mut fields, "derivedRoles")?;
    if roles.is_empty() && derived_roles.is_empty() {
        return Err(file.refuse(node.line, "a rule names `roles` or `derivedRoles`"));
    }
    let condition = fields
        .take("condition")
        .map(|n| parse_condition(file, n))
        .transpose()?;
    fields.finish(file, "a rule")?;
    Ok(Rule {
        line: node.line,
        actions,
        effect,
        roles,
        derived_roles,
        condition,
    })
}

pub(crate) fn derived_roles(file: &File, source: &str) -> Result<DerivedRoleSet, PolicyError> {
    let root = file.parse(source)?;
    let node = body(file, &root, Expect::DerivedRoles)?;
    let mut fields = file.map(node, "`derivedRoles`")?;
    let name = named(file, fields.require(file, "name")?, "`name`")?;
    let definitions = file
        .seq(fields.require(file, "definitions")?, "`definitions`")?
        .iter()
        .map(|n| {
            let mut fields = file.map(n, "a derived role")?;
            let name = named(file, fields.require(file, "name")?, "`name`")?;
            let parent_roles = non_empty(file, &mut fields, "parentRoles")?
                .into_iter()
                .map(|(r, _)| r)
                .collect();
            let condition = fields
                .take("condition")
                .map(|n| parse_condition(file, n))
                .transpose()?;
            fields.finish(file, "a derived role")?;
            Ok(DerivedRole {
                name,
                parent_roles,
                condition,
            })
        })
        .collect::<Result<_, _>>()?;
    fields.finish(file, "`derivedRoles`")?;
    Ok(DerivedRoleSet { name, definitions })
}

pub(crate) fn default_grants(file: &File, source: &str) -> Result<Vec<DefaultGrant>, PolicyError> {
    let root = file.parse(source)?;
    let node = body(file, &root, Expect::DefaultGrants)?;
    file.seq(node, "`defaultGrants`")?
        .iter()
        .map(|n| {
            let mut fields = file.map(n, "a default grant")?;
            let role = file
                .str(fields.require(file, "role")?, "`role`")?
                .to_owned();
            let resource = named(file, fields.require(file, "resource")?, "`resource`")?;
            let actions = match (fields.take("action"), fields.has("actions")) {
                (Some(action), false) => vec![named(file, action, "`action`")?],
                (None, true) => non_empty(file, &mut fields, "actions")?,
                _ => {
                    return Err(
                        file.refuse(n.line, "a default grant names one of `action` or `actions`")
                    );
                }
            };
            let scope = fields
                .take("scope")
                .map(|s| named(file, s, "`scope`"))
                .transpose()?;
            fields.finish(file, "a default grant")?;
            Ok(DefaultGrant {
                line: n.line,
                role,
                resource,
                actions,
                scope,
            })
        })
        .collect()
}

/// What a Module calls itself, its resources, their actions and their scopes, for admins.
#[derive(Debug, Clone, Default)]
pub(crate) struct Labels {
    pub module: Option<String>,
    pub resources: Vec<ResourceLabels>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResourceLabels {
    pub resource: Named,
    pub label: Option<String>,
    pub actions: Vec<(Named, String)>,
    pub scopes: Vec<(Named, String)>,
}

pub(crate) fn labels(file: &File, source: &str) -> Result<Labels, PolicyError> {
    let root = file.parse(source)?;
    let node = body(file, &root, Expect::Labels)?;
    let mut fields = file.map(node, "`labels`")?;
    let module = fields
        .take("module")
        .map(|n| text(file, n, "`module`"))
        .transpose()?;
    let resources = match fields.take("resources") {
        Some(n) => file
            .seq(n, "`resources`")?
            .iter()
            .map(|n| {
                let mut fields = file.map(n, "a resource's labels")?;
                let resource = named(file, fields.require(file, "resource")?, "`resource`")?;
                let label = fields
                    .take("label")
                    .map(|n| text(file, n, "`label`"))
                    .transpose()?;
                let actions = fields
                    .take("actions")
                    .map(|n| label_map(file, n, "`actions`"))
                    .transpose()?
                    .unwrap_or_default();
                let scopes = fields
                    .take("scopes")
                    .map(|n| label_map(file, n, "`scopes`"))
                    .transpose()?
                    .unwrap_or_default();
                fields.finish(file, "a resource's labels")?;
                Ok(ResourceLabels {
                    resource,
                    label,
                    actions,
                    scopes,
                })
            })
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };
    fields.finish(file, "`labels`")?;
    Ok(Labels { module, resources })
}

fn label_map(file: &File, node: &Node, what: &str) -> Result<Vec<(Named, String)>, PolicyError> {
    let entries = match &node.kind {
        Kind::Map(entries) => entries,
        other => {
            return Err(file.refuse(
                node.line,
                format!("{what} must be a mapping, not {}", other.describe()),
            ));
        }
    };
    let mut labels = Vec::new();
    for (key, value) in entries {
        if key.name.is_empty() {
            return Err(file.refuse(key.line, format!("each key of {what} must be a name")));
        }
        labels.push((
            (key.name.clone(), key.line),
            text(file, value, &format!("`{}`'s label", key.name))?,
        ));
    }
    Ok(labels)
}

fn text(file: &File, node: &Node, what: &str) -> Result<String, PolicyError> {
    match &node.kind {
        Kind::Str(s) if !s.trim().is_empty() => Ok(s.trim().to_owned()),
        _ => Err(file.refuse(node.line, format!("{what} must be some text"))),
    }
}

fn named(file: &File, node: &Node, what: &str) -> Result<Named, PolicyError> {
    match &node.kind {
        Kind::Str(s) if !s.is_empty() => Ok((s.clone(), node.line)),
        _ => Err(file.refuse(node.line, format!("{what} must be a name"))),
    }
}

fn optional_strings(
    file: &File,
    fields: &mut Fields,
    key: &str,
) -> Result<Vec<Named>, PolicyError> {
    fields
        .take(key)
        .map(|n| file.strings(n, &format!("`{key}`")))
        .transpose()
        .map(Option::unwrap_or_default)
}

fn non_empty(file: &File, fields: &mut Fields, key: &str) -> Result<Vec<Named>, PolicyError> {
    let node = fields.require(file, key)?;
    let items = file.strings(node, &format!("`{key}`"))?;
    if items.is_empty() {
        return Err(file.refuse(node.line, format!("`{key}` lists at least one")));
    }
    Ok(items)
}
