use super::events::{self, scope_name};
use super::{AccessService, Versioned};
use crate::domain::{Grant, GrantedBy};
use app_authz::{ADMIN, USER};
use app_error::AppResult;
use std::collections::{HashMap, HashSet};

/// What each Module's gate was named, before ADR 0047 removed gates.
const GATE_PREFIX: &str = "module:";

impl AccessService {
    /// Retires the Modules' gates (ADR 0047), once, in one unit of work: what `migrate` runs
    /// before it applies the defaults. Revokes, as `System` and announced, every grant on a
    /// Module's resources that its gate didn't let through, then every grant on a gate, so no
    /// role gains or loses what it could do. Returns how many it revoked.
    pub async fn retire_gates(&self) -> AppResult<usize> {
        let mut tx = self.transactions.begin().await?;
        // Taken first, so two `migrate`s retire them one after the other.
        self.repo.bump_revision(&mut tx).await?;
        if self.repo.gates_retired(&mut tx).await? {
            return Ok(0);
        }
        let grants = self.repo.all_versioned(&mut tx).await?;

        let mut passed: HashMap<&str, HashSet<&str>> = HashMap::new();
        for grant in &grants {
            if let Some(module) = grant.entity.resource().strip_prefix(GATE_PREFIX) {
                passed
                    .entry(module)
                    .or_default()
                    .insert(grant.entity.role());
            }
        }
        // The deleted `policies/gate.yaml` of `access`: admins always passed its gate.
        passed.entry("access").or_default().insert(ADMIN);

        let module_of = |resource: &str| {
            self.modules
                .policies
                .iter()
                .find(|p| p.has_resource(resource))
                .map(|p| p.module())
        };
        let let_through = |grant: &Grant| match module_of(grant.resource()) {
            // A grant no loaded policy has allowed nothing, and still allows nothing.
            None => true,
            Some(module) => passed.get(module).is_some_and(|roles| {
                // Every principal holds `user`, so its passing the gate let every role through.
                roles.contains(USER) || roles.contains(grant.role())
            }),
        };
        let (gates, resources): (Vec<_>, Vec<_>) = grants
            .iter()
            .partition(|g| g.entity.resource().starts_with(GATE_PREFIX));
        let revoked: Vec<_> = resources
            .into_iter()
            .filter(|g| !let_through(&g.entity))
            .chain(gates)
            .collect();
        for grant in &revoked {
            self.revoke_as_system(&mut tx, grant).await?;
        }

        self.repo
            .mark_gates_retired(&mut tx, self.clock.now())
            .await?;
        tx.commit().await?;
        Ok(revoked.len())
    }

    async fn revoke_as_system(
        &self,
        tx: &mut super::Tx,
        current: &Versioned<Grant>,
    ) -> AppResult<()> {
        let grant = &current.entity;
        let event = grant.revoke(GrantedBy::System);
        self.repo.delete(tx, grant.id(), current.version).await?;
        self.publisher
            .publish(tx, events::pending(grant, current.version.next(), event))
            .await?;
        tracing::info!(
            role = grant.role(),
            resource = grant.resource(),
            action = grant.action(),
            scope = scope_name(grant.scope()),
            "grant revoked as Modules lose their gates"
        );
        Ok(())
    }
}
