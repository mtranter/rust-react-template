use crate::driver::{Refusal, Widgets};
use sqlx::PgPool;
use widgets::seed::{FIRST_WIDGET, SECOND_WIDGET};

#[sqlx::test(migrations = false)]
async fn widgets_are_listed_most_recent_first(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    let page = widgets.list().await;
    assert_eq!(page.total, 2);
    assert_eq!(
        page.ids(),
        [SECOND_WIDGET.to_string(), FIRST_WIDGET.to_string()]
    );
}

#[sqlx::test(migrations = false)]
async fn widgets_can_be_sorted_by_name(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    let page = widgets
        .list_sorted(&["name"])
        .await
        .expect("a known sort key");
    assert_eq!(
        page.ids(),
        [FIRST_WIDGET.to_string(), SECOND_WIDGET.to_string()]
    );
}

#[sqlx::test(migrations = false)]
async fn sorting_by_an_unknown_key_is_malformed(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    assert_eq!(
        widgets.list_sorted(&["colour"]).await.err(),
        Some(Refusal::Malformed)
    );
}
