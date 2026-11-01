use crate::driver::{Refusal, Widgets};
use sqlx::PgPool;
use widgets::WidgetId;
use widgets::seed::{self, FIRST_WIDGET};

#[sqlx::test(migrations = false)]
async fn a_widget_can_be_found_by_its_id(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    assert_eq!(
        widgets.get(FIRST_WIDGET).await,
        Ok(seed::first_widget().into())
    );
}

#[sqlx::test(migrations = false)]
async fn an_unknown_widget_is_not_found(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    assert_eq!(widgets.get(WidgetId::new()).await, Err(Refusal::NotFound));
}

#[sqlx::test(migrations = false)]
async fn a_malformed_widget_id_is_invalid(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    assert_eq!(
        widgets.get("not-a-widget-id").await,
        Err(Refusal::invalid([]))
    );
}
