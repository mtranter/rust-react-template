use crate::driver::{Refusal, Widgets};
use sqlx::PgPool;
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
