use crate::driver::{Outcome, UiSettings};
use serde_json::json;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_theme_never_saved_is_version_zero_with_no_overrides(pool: PgPool) {
    let settings = UiSettings::seeded(pool).await;

    assert_eq!(
        settings.theme().await,
        Outcome::Ok(json!({
            "version": 0, "light": {}, "dark": {}, "shared": {},
            "updatedAt": null, "updatedBy": null
        }))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn anyone_reads_the_theme_without_signing_in(pool: PgPool) {
    let settings = UiSettings::seeded(pool).await;
    settings
        .update(
            0,
            json!({ "light": { "primary": "red" }, "dark": {}, "shared": {} }),
        )
        .await
        .unwrap();

    settings.sign_out();

    let theme = settings.theme().await.unwrap();
    assert_eq!(theme["light"], json!({ "primary": "red" }));
    assert_eq!(theme["version"], 1);
}
