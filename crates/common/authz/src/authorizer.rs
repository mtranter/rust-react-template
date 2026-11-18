use std::sync::Arc;

use arc_swap::ArcSwap;

use app_error::{AppError, AppResult};

use crate::{CompileError, Plan, Policies, Principal, Resource, RuleSet};

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

    /// Every Module's policies with their default grants, held in memory: until grants are stored
    /// and editable (platform ticket 0029), the defaults are the only grants.
    pub fn from_policies(policies: &[Policies]) -> Result<Self, CompileError> {
        let defaults: Vec<_> = policies
            .iter()
            .flat_map(|p| p.default_grants().iter().cloned())
            .collect();
        Ok(Self::new(RuleSet::compile(policies, &defaults)?.rule_set))
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

    /// The Module's gate, `module:<name>` / `use`.
    pub fn allows_use_of(&self, principal: &Principal, module: &str) -> bool {
        self.current.load().plan_gate(principal, module) == Plan::Allowed
    }
}
