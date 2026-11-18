use std::collections::{BTreeSet, HashMap};

use crate::condition::Condition;
use crate::policies::GATE_ACTION;
use crate::policy::Effect;
use crate::{Action, Filter, Grant, Policies, Principal, Resource, USER};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Roles {
    Any,
    Named(Vec<String>),
}

impl From<Vec<String>> for Roles {
    fn from(roles: Vec<String>) -> Self {
        if roles.iter().any(|r| r == "*") {
            Roles::Any
        } else {
            Roles::Named(roles)
        }
    }
}

impl Roles {
    fn include(&self, role: &str) -> bool {
        match self {
            Roles::Any => true,
            Roles::Named(roles) => roles.iter().any(|r| r == role),
        }
    }
}

/// A rule with its wildcards and derived roles expanded: a grant compiles to one too.
#[derive(Debug, Clone)]
pub(crate) struct Rule {
    pub actions: Vec<&'static str>,
    pub roles: Roles,
    pub effect: Effect,
    pub condition: Option<Condition>,
}

#[derive(Debug, Clone)]
struct Entry {
    roles: Roles,
    effect: Effect,
    condition: Option<Condition>,
}

/// What a principal may do with an action: everywhere, nowhere, or on the rows the filter matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Allowed,
    Denied,
    Where(Filter),
}

/// The rows a list may hold: a plan that isn't `Denied`, which the service has already refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rows {
    All,
    Where(Filter),
}

impl Rows {
    pub fn allows<R: Resource>(&self, resource: &R) -> bool {
        match self {
            Rows::All => true,
            Rows::Where(filter) => filter.matches(resource),
        }
    }
}

impl Plan {
    /// `None` when `Denied`.
    pub fn rows(self) -> Option<Rows> {
        match self {
            Plan::Allowed => Some(Rows::All),
            Plan::Denied => None,
            Plan::Where(filter) => Some(Rows::Where(filter)),
        }
    }

    pub fn allows<R: Resource>(&self, resource: &R) -> bool {
        match self {
            Plan::Allowed => true,
            Plan::Denied => false,
            Plan::Where(filter) => filter.matches(resource),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CompileError {
    #[error("resource `{0}` is in more than one module's policies")]
    DuplicateResource(String),
}

/// A compiled rule set, and the grants it skipped because they no longer check (a later release
/// removed their action or scope).
#[derive(Debug, Clone)]
pub struct Compiled {
    pub rule_set: RuleSet,
    pub stale: Vec<Grant>,
}

/// Every Module's policies and every grant, compiled into one set of rules keyed by resource and
/// action, with the roles assigned to each subject. Rebuilt and swapped whole when grants, roles
/// or assignments change (`Authorizer`), so resolving a Principal reads no store.
#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    rules: HashMap<String, HashMap<&'static str, Vec<Entry>>>,
    assigned: HashMap<String, BTreeSet<String>>,
}

impl RuleSet {
    pub fn compile(policies: &[Policies], grants: &[Grant]) -> Result<Compiled, CompileError> {
        let mut rule_set = RuleSet::default();
        for p in policies {
            for resource in p.resources() {
                if rule_set.rules.contains_key(&resource.kind) {
                    return Err(CompileError::DuplicateResource(resource.kind.clone()));
                }
                rule_set.rules.insert(resource.kind.clone(), HashMap::new());
                for rule in &resource.rules {
                    rule_set.add(&resource.kind, rule);
                }
            }
        }
        let mut stale = Vec::new();
        for grant in grants {
            match policies
                .iter()
                .find(|p| p.resource(&grant.resource).is_some())
                .map(|p| p.grant_rule(grant))
            {
                Some(Ok(rule)) => rule_set.add(&grant.resource, &rule),
                _ => stale.push(grant.clone()),
            }
        }
        Ok(Compiled { rule_set, stale })
    }

    /// These rules, with `assignments` (subject, role) as the roles each subject holds beside
    /// [`USER`].
    pub fn assigning(mut self, assignments: impl IntoIterator<Item = (String, String)>) -> Self {
        self.assigned.clear();
        for (subject, role) in assignments {
            self.assign(subject, role);
        }
        self
    }

    pub(crate) fn assign(&mut self, subject: String, role: String) {
        if role != USER {
            self.assigned.entry(subject).or_default().insert(role);
        }
    }

    pub(crate) fn unassign(&mut self, subject: &str) {
        self.assigned.remove(subject);
    }

    /// The roles assigned to `subject`, by name; [`USER`] is implicit, so never among them.
    pub fn assigned(&self, subject: &str) -> impl Iterator<Item = &str> {
        self.assigned
            .get(subject)
            .into_iter()
            .flatten()
            .map(String::as_str)
    }

    fn add(&mut self, kind: &str, rule: &Rule) {
        let actions = self.rules.entry(kind.to_owned()).or_default();
        for action in &rule.actions {
            actions.entry(action).or_default().push(Entry {
                roles: rule.roles.clone(),
                effect: rule.effect,
                condition: rule.condition.clone(),
            });
        }
    }

    pub fn plan<R: Resource>(&self, principal: &Principal, action: R::Action) -> Plan {
        self.plan_named(principal, R::KIND, action.name())
    }

    /// The Module's gate: `Allowed` or `Denied`, since it takes no condition.
    pub fn plan_gate(&self, principal: &Principal, module: &str) -> Plan {
        self.plan_named(principal, &format!("module:{module}"), GATE_ACTION)
    }

    /// Anything not allowed is denied. Within one role, a matching DENY beats an ALLOW; across
    /// roles, an ALLOW from any role wins (ADR 0040).
    pub fn plan_named(&self, principal: &Principal, kind: &str, action: &str) -> Plan {
        let Principal::User { roles, .. } = principal else {
            return Plan::Allowed;
        };
        let entries = self
            .rules
            .get(kind)
            .and_then(|actions| actions.get(action))
            .map_or(&[][..], Vec::as_slice);
        let resolve = |e: &Entry| {
            e.condition
                .as_ref()
                .map_or(Filter::True, |c| c.resolve(principal))
        };
        let per_role = roles.iter().map(|role| {
            let (allows, denies): (Vec<&Entry>, Vec<&Entry>) = entries
                .iter()
                .filter(|e| e.roles.include(role))
                .partition(|e| e.effect == Effect::Allow);
            Filter::all(vec![
                Filter::any(allows.into_iter().map(resolve).collect()),
                Filter::none(denies.into_iter().map(resolve).collect()),
            ])
        });
        match Filter::any(per_role.collect()) {
            Filter::True => Plan::Allowed,
            Filter::False => Plan::Denied,
            filter => Plan::Where(filter),
        }
    }

    /// Whether any rule could allow the action, for some role and some row.
    pub fn could_allow(&self, kind: &str, action: &str) -> bool {
        self.rules
            .get(kind)
            .and_then(|actions| actions.get(action))
            .is_some_and(|entries| entries.iter().any(|e| e.effect == Effect::Allow))
    }
}
