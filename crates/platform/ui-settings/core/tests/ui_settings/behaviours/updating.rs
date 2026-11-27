use crate::driver::{Refusal, SUBJECT, UiSettings};
use app_events::EventPayload;
use app_persistence::Version;
use serde_json::json;
use sqlx::PgPool;
use std::collections::BTreeMap;
use ui_settings_api::events::v1::{self, ThemeUpdated};

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_admin_saves_the_theme_and_reads_it_back(pool: PgPool) {
    let settings = UiSettings::seeded(pool).await;

    let saved = settings
        .update(
            0,
            json!({
                "light": { "primary": " oklch(0.55 0.2 250) " },
                "dark": { "primary": "oklch(0.7 0.15 250)" },
                "shared": { "radius": "0.25rem", "font-sans": "\"Inter\", sans-serif" }
            }),
        )
        .await
        .unwrap();

    assert_eq!(saved["version"], 1);
    assert_eq!(saved["light"]["primary"], "oklch(0.55 0.2 250)");
    assert_eq!(saved["shared"]["font-sans"], "\"Inter\", sans-serif");
    assert_eq!(saved["updatedBy"], SUBJECT);
    assert!(saved["updatedAt"].is_string());
    assert_eq!(settings.theme().await, Ok(saved));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn saving_empty_maps_resets_to_the_defaults(pool: PgPool) {
    let settings = UiSettings::seeded(pool).await;
    settings
        .update(
            0,
            json!({ "light": { "primary": "red" }, "dark": {}, "shared": {} }),
        )
        .await
        .unwrap();

    let reset = settings
        .update(1, json!({ "light": {}, "dark": {}, "shared": {} }))
        .await
        .unwrap();

    assert_eq!(reset["version"], 2);
    assert_eq!(reset["light"], json!({}));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_stale_version_changes_nothing(pool: PgPool) {
    let settings = UiSettings::seeded(pool).await;
    let theme = || json!({ "light": { "primary": "red" }, "dark": {}, "shared": {} });
    settings.update(0, theme()).await.unwrap();

    assert_eq!(settings.update(0, theme()).await, Err(Refusal::Stale));
    assert_eq!(settings.update(5, theme()).await, Err(Refusal::Stale));
    assert_eq!(
        settings.update_without_precondition(theme()).await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(settings.theme().await.unwrap()["version"], 1);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn bad_overrides_are_refused_by_their_paths(pool: PgPool) {
    let settings = UiSettings::seeded(pool).await;

    let refused = settings
        .update(
            0,
            json!({
                "light": { "primary": "red; background: url(x)", "radius": "1rem" },
                "dark": { "ring": "" },
                "shared": { "font-sans": "</style>", "primary": "red" }
            }),
        )
        .await;

    assert_eq!(
        refused,
        Err(Refusal::Invalid(
            [
                "light.primary",
                "light.radius",
                "dark.ring",
                "shared.font-sans",
                "shared.primary"
            ]
            .map(String::from)
            .to_vec()
        ))
    );
    assert_eq!(settings.theme().await.unwrap()["version"], 0);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn saving_the_theme_announces_it_as_it_now_is(pool: PgPool) {
    let settings = UiSettings::seeded(pool).await;
    let saved = settings
        .update(
            0,
            json!({ "light": {}, "dark": {}, "shared": { "radius": "0" } }),
        )
        .await
        .unwrap();

    let announced = settings.announced::<ThemeUpdated>().await;
    assert_eq!(
        announced,
        [(
            Version::from_i64(1),
            EventPayload::new(v1::Theme {
                light: BTreeMap::new(),
                dark: BTreeMap::new(),
                shared: BTreeMap::from([("radius".into(), "0".into())]),
                updated_at: saved["updatedAt"].as_str().unwrap().parse().unwrap(),
                updated_by: SUBJECT.into(),
            })
        )]
    );
}
