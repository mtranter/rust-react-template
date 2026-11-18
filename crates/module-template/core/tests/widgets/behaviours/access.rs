use crate::driver::{Refusal, Widgets};
use app_authz::Principal;
use app_db::Columns;
use sqlx::PgPool;
use widgets::{WidgetColumns, seed};
use widgets_api::events::v1::WidgetCreated;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn widgets_are_refused_to_a_caller_who_is_not_signed_in(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    widgets.sign_out();

    assert_eq!(widgets.create("Sprocket").await, Err(Refusal::NotSignedIn));
    assert!(widgets.announced::<WidgetCreated>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_signed_in_caller_without_the_user_role_is_kept_out_of_widgets(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    widgets.sign_in_with_roles(&["guest"]);

    assert_eq!(widgets.create("Sprocket").await, Err(Refusal::Forbidden));
    assert!(widgets.announced::<WidgetCreated>().await.is_empty());
}

#[test]
fn every_widget_attribute_has_a_column() {
    WidgetColumns::assert_every_attribute_is_mapped();
}

/// Keeps the list's SQL and the in-memory check of one widget from drifting
/// apart, for every action and caller: rerun it after adding a scope or rule.
#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_scoped_query_holds_exactly_the_widgets_each_plan_allows(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    let principals = [
        Principal::user("someone", [crate::driver::USER]),
        Principal::user("someone", ["admin"]),
        Principal::user("someone", ["guest"]),
    ];

    app_authz::testing::assert_lists_agree(
        &widgets.authorizer(),
        &principals,
        &seed::widgets(),
        |widget| widget.id().as_uuid(),
        |scope| widgets.ids_where::<WidgetColumns>(scope),
    )
    .await;
}
