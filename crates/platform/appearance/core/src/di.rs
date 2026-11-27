// The composition root is where the pool meets the repository (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::ThemeService;
use crate::db::{PgThemeRepository, migrations};
use crate::domain::Theme;
use crate::web::OPERATIONS;
use app_authz::{Policies, PoliciesBuilder, policy_file};
use app_module::{Context, Module};
use app_web::Provide;
use appearance_api::events::v1::ThemeTopic;
use std::sync::Arc;

pub struct AppearanceProvider(ThemeService);

impl Provide for AppearanceProvider {
    type Data = ThemeService;

    fn build(self) -> ThemeService {
        self.0
    }
}

pub fn provider(ctx: &Context) -> AppearanceProvider {
    AppearanceProvider(ThemeService::new(
        Arc::new(PgThemeRepository::new(ctx.pool())),
        ctx.publisher(),
        ctx.transactions(),
        ctx.clock(),
        ctx.authorizer(),
    ))
}

/// The `theme` resource, `admin`'s by default (ADR 0051: no role of its own).
pub fn policies() -> PoliciesBuilder {
    Policies::builder("appearance")
        .capability(policy_file!("../capability.yaml"))
        .resource::<Theme>(policy_file!("policies/theme.yaml"))
        .default_grants(policy_file!("policies/default_grants.yaml"))
        .labels(policy_file!("policies/labels.yaml"))
}

pub fn module() -> Module {
    Module::new(provider)
        .with_policies(policies)
        .performing(OPERATIONS)
        .publishes::<ThemeTopic>()
        .with_migrations(migrations())
}
