//! Ids are `well_known_id(MODULE, kind, sequence)` (spec §13); kind `1` is a widget.

// Seeding wires a pool to a repository, like `di.rs`.
#![allow(clippy::disallowed_types)]

use crate::application::{Expected, WidgetRepository};
use crate::db::PgWidgetRepository;
use crate::domain::{CreateWidget, Widget, WidgetId};
use app_domain::{seed_time, well_known_id};
use app_error::AppResult;
use sqlx::PgPool;

/// Unique across modules; `make check-modules` enforces it.
pub const MODULE: u32 = 0;

pub const FIRST_WIDGET: WidgetId = WidgetId::from_uuid(well_known_id(MODULE, 1, 1));
pub const SECOND_WIDGET: WidgetId = WidgetId::from_uuid(well_known_id(MODULE, 1, 2));

fn widget(id: WidgetId, name: &str, minutes: i64) -> Widget {
    Widget::create(id, CreateWidget { name: name.into() }, seed_time(minutes))
        .expect("seed data satisfies the domain's invariants")
}

pub fn first_widget() -> Widget {
    widget(FIRST_WIDGET, "First", 0)
}

pub fn second_widget() -> Widget {
    widget(SECOND_WIDGET, "Second", 1)
}

pub fn widgets() -> Vec<Widget> {
    vec![first_widget(), second_widget()]
}

/// Saves only what's missing, so a deployed environment can re-run it on every
/// release without failing or undoing edits to seeded entities.
pub async fn standard(pool: &PgPool) -> AppResult<()> {
    let repo = PgWidgetRepository::new(pool.clone());
    for widget in widgets() {
        if repo.get(widget.id()).await?.is_none() {
            repo.save(&widget, Expected::New).await?;
        }
    }
    Ok(())
}
