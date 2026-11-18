use std::sync::Arc;

use arc_swap::ArcSwap;

use app_error::{AppError, AppResult};

use crate::{CompileError, Plan, Policies, Principal, Resource, Rows, RuleSet};

/// The rule set in force, replaced whole when grants change.
#[derive(Debug)]
pub struct Authorizer {
    current: ArcSwap<RuleSet>,
}

impl Authorizer {
    pub fn new(rule_set: RuleSet) -> Self {
        Self {
            current: ArcSwap::from_pointee(rule_set),
        }
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
