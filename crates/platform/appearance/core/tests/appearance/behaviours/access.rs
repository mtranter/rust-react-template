use crate::driver::{Appearance, Refusal};
use serde_json::json;
use sqlx::PgPool;

fn reset() -> serde_json::Value {
    json!({ "light": {}, "dark": {}, "shared": {} })
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_who_is_not_an_admin_may_not_change_the_theme(pool: PgPool) {
    let appearance = Appearance::seeded(pool).await;
    appearance.sign_in_as_a_user();

    assert_eq!(appearance.update(0, reset()).await, Err(Refusal::Forbidden));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn changing_the_theme_needs_a_sign_in(pool: PgPool) {
    let appearance = Appearance::seeded(pool).await;
    appearance.sign_out();

    assert_eq!(
        appearance.update(0, reset()).await,
        Err(Refusal::NotSignedIn)
    );
}
