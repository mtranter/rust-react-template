use crate::driver::{Appearance, Refusal, SUBJECT};
use app_events::EventPayload;
use app_persistence::Version;
use appearance_api::events::v1::{self, AppVisibilityUpdated};
use serde_json::json;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn no_app_is_hidden_until_an_admin_hides_one(pool: PgPool) {
    let appearance = Appearance::seeded(pool).await;
    appearance.sign_in_as_a_user();

    assert_eq!(
        appearance.apps().await,
        Ok(json!({ "version": 0, "hidden": [], "updatedAt": null, "updatedBy": null }))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_admin_hides_apps_and_any_user_reads_which(pool: PgPool) {
    let appearance = Appearance::seeded(pool).await;

    let saved = appearance.hide_apps(0, &["notes", "crm"]).await.unwrap();
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["hidden"], json!(["crm", "notes"]));
    assert_eq!(saved["updatedBy"], SUBJECT);

    appearance.sign_in_as_a_user();
    assert_eq!(appearance.apps().await, Ok(saved));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_empty_list_shows_every_app_again(pool: PgPool) {
    let appearance = Appearance::seeded(pool).await;
    appearance.hide_apps(0, &["notes"]).await.unwrap();

    let shown = appearance.hide_apps(1, &[]).await.unwrap();

    assert_eq!(shown["version"], 2);
    assert_eq!(shown["hidden"], json!([]));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_stale_version_hides_nothing(pool: PgPool) {
    let appearance = Appearance::seeded(pool).await;
    appearance.hide_apps(0, &["notes"]).await.unwrap();

    assert_eq!(appearance.hide_apps(0, &["crm"]).await, Err(Refusal::Stale));
    assert_eq!(appearance.hide_apps(4, &["crm"]).await, Err(Refusal::Stale));
    assert_eq!(appearance.apps().await.unwrap()["hidden"], json!(["notes"]));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn ids_that_name_no_app_are_refused_by_their_paths(pool: PgPool) {
    let appearance = Appearance::seeded(pool).await;

    assert_eq!(
        appearance.hide_apps(0, &["notes", "Not An Id", ""]).await,
        Err(Refusal::Invalid(vec!["hidden.1".into(), "hidden.2".into()]))
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_who_is_not_an_admin_may_not_hide_apps(pool: PgPool) {
    let appearance = Appearance::seeded(pool).await;
    appearance.sign_in_as_a_user();

    assert_eq!(
        appearance.hide_apps(0, &["notes"]).await,
        Err(Refusal::Forbidden)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn reading_which_apps_are_hidden_needs_a_sign_in(pool: PgPool) {
    let appearance = Appearance::seeded(pool).await;
    appearance.sign_out();

    assert_eq!(appearance.apps().await, Err(Refusal::NotSignedIn));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn hiding_apps_announces_which_are_now_hidden(pool: PgPool) {
    let appearance = Appearance::seeded(pool).await;
    let saved = appearance.hide_apps(0, &["notes"]).await.unwrap();

    assert_eq!(
        appearance.announced::<AppVisibilityUpdated>().await,
        [(
            Version::from_i64(1),
            EventPayload::new(v1::AppVisibility {
                hidden: vec!["notes".into()],
                updated_at: saved["updatedAt"].as_str().unwrap().parse().unwrap(),
                updated_by: SUBJECT.into(),
            })
        )]
    );
}
