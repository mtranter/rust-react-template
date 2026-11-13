use crate::driver::{Refusal, Widgets};
use sqlx::PgPool;
use widgets::WidgetId;
use widgets::seed::{self, WIDGET_ID_FIRST};

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_widget_can_be_found_by_its_id(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    assert_eq!(
        widgets.get(WIDGET_ID_FIRST).await,
        Ok(seed::first_widget().into())
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_unknown_widget_is_not_found(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    assert_eq!(widgets.get(WidgetId::new()).await, Err(Refusal::NotFound));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_malformed_widget_id_is_invalid(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    assert_eq!(
        widgets.get("not-a-widget-id").await,
        Err(Refusal::invalid([]))
    );
}
