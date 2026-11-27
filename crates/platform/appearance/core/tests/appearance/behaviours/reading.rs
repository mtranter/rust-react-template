use crate::driver::{Appearance, Outcome};
use serde_json::json;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_theme_never_saved_is_version_zero_with_no_overrides(pool: PgPool) {
    let appearance = Appearance::seeded(pool).await;

    assert_eq!(
        appearance.theme().await,
        Outcome::Ok(json!({
            "version": 0, "light": {}, "dark": {}, "shared": {},
            "updatedAt": null, "updatedBy": null
        }))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn anyone_reads_the_theme_without_signing_in(pool: PgPool) {
    let appearance = Appearance::seeded(pool).await;
    appearance
        .update(
            0,
            json!({ "light": { "primary": "red" }, "dark": {}, "shared": {} }),
        )
        .await
        .unwrap();

    appearance.sign_out();

    let theme = appearance.theme().await.unwrap();
    assert_eq!(theme["light"], json!({ "primary": "red" }));
    assert_eq!(theme["version"], 1);
}
