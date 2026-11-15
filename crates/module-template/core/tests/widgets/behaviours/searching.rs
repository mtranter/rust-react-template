use crate::driver::{Refusal, WidgetMatchJson, Widgets};
use sqlx::PgPool;
use widgets::seed::{self, WIDGET_ID_FIRST};

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn nothing_is_found_until_the_copy_has_caught_up(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    assert_eq!(
        widgets.search("first").await.err(),
        Some(Refusal::Unavailable)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn widgets_published_before_the_copy_existed_are_found_by_the_start_of_their_name(
    pool: PgPool,
) {
    let widgets = Widgets::seeded(pool).await;
    widgets.run_pending_events().await;

    let found = widgets.search("FIR").await.unwrap();

    assert_eq!(found.items, [WidgetMatchJson::from(seed::first_widget())]);
    assert_eq!(found.total, 1);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_renamed_widget_is_found_by_its_new_name_once_its_event_is_delivered(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    widgets.run_pending_events().await;

    widgets.rename(WIDGET_ID_FIRST, 1, "Renamed").await.unwrap();
    assert!(widgets.search("renamed").await.unwrap().items.is_empty());

    widgets.run_pending_events().await;
    assert_eq!(
        widgets.search("renamed").await.unwrap().ids(),
        [WIDGET_ID_FIRST.to_string()]
    );
    assert!(widgets.search("first").await.unwrap().items.is_empty());
}
