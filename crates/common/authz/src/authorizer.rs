use std::sync::Arc;

use arc_swap::ArcSwap;

use app_error::{AppError, AppResult};

use crate::{ADMIN, CompileError, Plan, Policies, Principal, Resource, Rows, RuleSet, USER};

/// The rule set in force, replaced whole when grants, roles or assignments change, and the
/// bootstrap admins, fixed for the process's life.
#[derive(Debug)]
pub struct Authorizer {
    current: ArcSwap<RuleSet>,
    bootstrap_admins: Vec<String>,
}

impl Authorizer {
    pub fn new(rule_set: RuleSet) -> Self {
        Self {
            current: ArcSwap::from_pointee(rule_set),
            bootstrap_admins: Vec::new(),
        }
    }

    /// `subjects` hold [`ADMIN`] whatever is assigned (`APP_BOOTSTRAP_ADMINS`): how a fresh app
    /// gets its first admin.
    pub fn with_bootstrap_admins(self, subjects: impl IntoIterator<Item = String>) -> Self {
        Self {
            bootstrap_admins: subjects.into_iter().collect(),
            ..self
        }
    }

    pub fn bootstrap_admins(&self) -> &[String] {
        &self.bootstrap_admins
    }

    pub fn is_bootstrap_admin(&self, subject: &str) -> bool {
        self.bootstrap_admins.iter().any(|s| s == subject)
    }

    /// The user `subject` acts as: [`USER`], the roles assigned to it in the rule set in force,
    /// and [`ADMIN`] if it is a bootstrap admin. Reads no store.
    pub fn principal(&self, subject: &str) -> Principal {
        let current = self.current.load();
        let mut roles = vec![USER];
        roles.extend(current.assigned(subject));
        if self.is_bootstrap_admin(subject) && !roles.contains(&ADMIN) {
            roles.push(ADMIN);
        }
        Principal::user(subject, roles)
    }

    /// Assigns `subject` exactly `roles` (beside [`USER`]) in the rule set in force, until it is
    /// next replaced: for a Module's own tests, which have no `access` to assign them.
    #[cfg(any(test, feature = "testing"))]
    pub fn assign_in_memory(&self, subject: &str, roles: &[&str]) {
        self.current.rcu(|current| {
            let mut next = RuleSet::clone(current);
            next.unassign(subject);
            for role in roles {
                next.assign(subject.to_owned(), (*role).to_owned());
            }
            next
        });
    }

    /// Every Module's policies with their default grants, held in memory: for a Module's own
    /// tests, which run without the `access` Module that stores grants.
    pub fn from_policies(policies: &[Policies]) -> Result<Self, CompileError> {
        let defaults: Vec<_> = policies
            .iter()
            .flat_map(|p| p.default_grants().iter().cloned())
            .collect();
        Ok(Self::new(RuleSet::compile(policies, &defaults)?.rule_set))
    }

    /// Every Module's baseline rules and no grant: what a process holds until it has read the
    /// stored grants, and all the worker needs, acting as `System`.
    pub fn baseline(policies: &[Policies]) -> Result<Self, CompileError> {
        Ok(Self::new(RuleSet::compile(policies, &[])?.rule_set))
    }

    /// A snapshot: a `replace` after this isn't seen through it.
    pub fn current(&self) -> Arc<RuleSet> {
        self.current.load_full()
    }

    pub fn replace(&self, rule_set: RuleSet) {
        self.current.store(Arc::new(rule_set));
    }

    pub fn plan<R: Resource>(&self, principal: &Principal, action: R::Action) -> Plan {
        self.current.load().plan::<R>(principal, action)
    }

    /// The action's plan, or `Forbidden` when it is `Denied`: what a use case asks before it loads
    /// or begins anything. A `Where` is returned for the caller to check against its row.
    pub fn authorize<R: Resource>(
        &self,
        principal: &Principal,
        action: R::Action,
    ) -> AppResult<Plan> {
        match self.plan::<R>(principal, action) {
            Plan::Denied => Err(AppError::Forbidden),
            plan => Ok(plan),
        }
    }

    /// The rows a list of `R` may hold, or `Forbidden` when it may hold none: what a list's use
    /// case asks before it queries, handing the answer to its repository.
    pub fn rows<R: Resource>(&self, principal: &Principal, action: R::Action) -> AppResult<Rows> {
        self.plan::<R>(principal, action)
            .rows()
            .ok_or(AppError::Forbidden)
    }

    /// Whether `plan`, from [`authorize`](Self::authorize), allows the loaded `row`: asked before
    /// any domain action on it. Refused, it is `NotFound` when the principal may not view the row
    /// either, so its existence isn't revealed, and `Forbidden` when they may (ADR 0040).
    pub fn check<R: Resource>(&self, principal: &Principal, plan: &Plan, row: &R) -> AppResult<()> {
        if plan.allows(row) {
            return Ok(());
        }
        let visible = R::VIEW.is_some_and(|view| self.plan::<R>(principal, view).allows(row));
        Err(if visible {
            AppError::Forbidden
        } else {
            AppError::not_found(R::KIND)
        })
    }

    /// The Module's gate, `module:<name>` / `use`.
    pub fn allows_use_of(&self, principal: &Principal, module: &str) -> bool {
        self.current.load().plan_gate(principal, module) == Plan::Allowed
    }
}
