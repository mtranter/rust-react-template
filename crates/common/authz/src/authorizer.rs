use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::{Plan, Principal, Resource, RuleSet};

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
}
