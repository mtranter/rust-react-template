use super::GrantRepository;
use super::service::Modules;
use app_authz::{Authorizer, RuleSet};
use app_error::{AppError, AppResult};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How often each server instance reads the revision (ADR 0040): a change of grants takes effect
/// everywhere within it.
pub const POLL_EVERY: Duration = Duration::from_secs(5);

/// Keeps a process's rule set in step with the stored grants: every module's policies and every
/// grant, compiled and swapped in whole when the revision has moved.
pub struct Rules {
    repo: Arc<dyn GrantRepository>,
    modules: Arc<Modules>,
    authorizer: Arc<Authorizer>,
    built_at: Mutex<Option<i64>>,
}

impl Rules {
    pub(crate) fn new(
        repo: Arc<dyn GrantRepository>,
        modules: Arc<Modules>,
        authorizer: Arc<Authorizer>,
    ) -> Self {
        Self {
            repo,
            modules,
            authorizer,
            built_at: Mutex::new(None),
        }
    }

    /// One poll: rebuilds the rule set if the revision has moved since it was last built. A
    /// failed read or compile changes nothing. A grant its module's policies no longer have (a
    /// later release removed its action or scope) is skipped, and logged once per revision.
    pub async fn refresh(&self) -> AppResult<()> {
        // The revision is read before the grants: a change committed between the two reads is
        // then built now and again on the next poll, never missed.
        let revision = self.repo.revision().await?;
        if *self.built_at.lock().expect("not poisoned") == Some(revision) {
            return Ok(());
        }
        let grants: Vec<_> = self
            .repo
            .all()
            .await?
            .iter()
            .map(|grant| grant.rule().clone())
            .collect();
        let compiled =
            RuleSet::compile(&self.modules.policies, &grants).map_err(AppError::internal)?;
        for stale in &compiled.stale {
            tracing::warn!(
                revision,
                grant = %stale,
                "a stored grant names what its module's policies no longer have: skipped until it is revoked"
            );
        }
        self.authorizer.replace(compiled.rule_set);
        *self.built_at.lock().expect("not poisoned") = Some(revision);
        tracing::info!(revision, grants = grants.len(), "rule set rebuilt");
        Ok(())
    }

    /// Polls every `every` for as long as it runs, keeping the rule set it has when a poll fails.
    pub async fn watch(&self, every: Duration) {
        let mut ticks = tokio::time::interval(every);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            if let Err(err) = self.refresh().await {
                tracing::error!(error = ?err, "rebuilding the rule set failed: keeping the one in force");
            }
        }
    }
}
