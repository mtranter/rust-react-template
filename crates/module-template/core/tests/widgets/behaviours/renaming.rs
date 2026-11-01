use crate::driver::{Refusal, Widgets, announced};
use app_events::EventPayload;
use app_persistence::Version;
use app_test_support::assert_json_matches;
use sqlx::PgPool;
use widgets::seed::{self, FIRST_WIDGET};
use widgets_api::events::v1::{self, WidgetRenamed};

#[sqlx::test(migrations = false)]
async fn renaming_a_widget_replaces_its_name_and_advances_its_version(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;

    let renamed = widgets.rename(FIRST_WIDGET, 1, " Renamed ").await.unwrap();

    assert_json_matches!(renamed, "$.name", "Renamed");
    assert_json_matches!(renamed, "$.version", "2");
    assert_eq!(widgets.get(FIRST_WIDGET).await, Ok(renamed));
}

#[sqlx::test(migrations = false)]
async fn renaming_a_widget_announces_it_as_it_now_is(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;

    widgets.rename(FIRST_WIDGET, 1, "Renamed").await.unwrap();

    assert_eq!(
        widgets.announced::<WidgetRenamed>().await,
        [(
            Version::from_i64(2),
            EventPayload::new(v1::Widget {
                name: "Renamed".into(),
                ..announced(&seed::first_widget()).entity
            })
        )]
    );
}

#[sqlx::test(migrations = false)]
async fn a_rename_without_a_name_is_rejected_and_changes_nothing(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    assert_eq!(
        widgets.rename(FIRST_WIDGET, 1, "").await,
        Err(Refusal::invalid(["name"]))
    );
    assert_eq!(
        widgets.get(FIRST_WIDGET).await,
        Ok(seed::first_widget().into())
    );
}

#[sqlx::test(migrations = false)]
async fn a_rename_against_a_stale_version_is_refused_and_changes_nothing(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    assert_eq!(
        widgets.rename(FIRST_WIDGET, 99, "Renamed").await,
        Err(Refusal::Stale)
    );
    assert_eq!(
        widgets.get(FIRST_WIDGET).await,
        Ok(seed::first_widget().into())
    );
    assert!(widgets.announced::<WidgetRenamed>().await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn a_rename_without_a_precondition_is_refused_and_changes_nothing(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;
    assert_eq!(
        widgets
            .rename_without_precondition(FIRST_WIDGET, "Renamed")
            .await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(
        widgets.get(FIRST_WIDGET).await,
        Ok(seed::first_widget().into())
    );
}
