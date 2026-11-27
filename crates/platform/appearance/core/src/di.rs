// The composition root is where the pool meets the repository (ADR 0007).
#![allow(clippy::disallowed_types)]

use crate::application::{AppVisibilityService, ThemeService};
use crate::db::{PgAppVisibilityRepository, PgThemeRepository, migrations};
use crate::domain::{AppVisibility, Theme};
use crate::web::OPERATIONS;
use app_authz::{Policies, PoliciesBuilder, policy_file};
use app_module::{Context, Module};
use app_web::Provide;
use appearance_api::events::v1::{AppVisibilityTopic, ThemeTopic};
use std::sync::Arc;

/// A Module provides one `Data`, so both of its services travel together.
#[derive(Clone)]
pub struct Appearance {
    pub themes: ThemeService,
    pub apps: AppVisibilityService,
}

pub struct AppearanceProvider(Appearance);

impl Provide for AppearanceProvider {
    type Data = Appearance;

    fn build(self) -> Appearance {
        self.0
    }
}

pub fn provider(ctx: &Context) -> AppearanceProvider {
    AppearanceProvider(Appearance {
        themes: ThemeService::new(
            Arc::new(PgThemeRepository::new(ctx.pool())),
            ctx.publisher(),
            ctx.transactions(),
            ctx.clock(),
            ctx.authorizer(),
        ),
        apps: AppVisibilityService::new(
            Arc::new(PgAppVisibilityRepository::new(ctx.pool())),
            ctx.publisher(),
            ctx.transactions(),
            ctx.clock(),
            ctx.authorizer(),
        ),
    })
}

/// The `theme` and `app_visibility` resources, `admin`'s by default (ADR 0051, 0052: no role of
/// their own).
pub fn policies() -> PoliciesBuilder {
    Policies::builder("appearance")
        .capability(policy_file!("../capability.yaml"))
        .resource::<Theme>(policy_file!("policies/theme.yaml"))
        .resource::<AppVisibility>(policy_file!("policies/app_visibility.yaml"))
        .default_grants(policy_file!("policies/default_grants.yaml"))
        .labels(policy_file!("policies/labels.yaml"))
}

pub fn module() -> Module {
    Module::new(provider)
        .with_policies(policies)
        .performing(OPERATIONS)
        .publishes::<ThemeTopic>()
        .publishes::<AppVisibilityTopic>()
        .with_migrations(migrations())
}
