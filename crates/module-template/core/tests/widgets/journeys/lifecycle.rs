use crate::driver::{Refusal, Widgets};
use app_persistence::Version;
use app_test_support::assert_json_matches;
use sqlx::PgPool;
use widgets_api::events::v1::{WidgetCreated, WidgetRenamed};

#[sqlx::test(migrations = false)]
async fn a_widget_can_be_made_and_renamed(pool: PgPool) {
    let widgets = Widgets::seeded(pool).await;

    let made = widgets.create("Sprocket").await.unwrap();
    assert_eq!(widgets.get(made.id()).await.as_ref(), Ok(&made));

    let renamed = widgets
        .rename(made.id(), made.version(), "Cog")
        .await
        .unwrap();
    assert_json_matches!(renamed, "$.name", "Cog");
    assert_eq!(widgets.get(made.id()).await.as_ref(), Ok(&renamed));

    assert_eq!(
        widgets.rename(made.id(), made.version(), "Gear").await,
        Err(Refusal::Stale)
    );
    assert_eq!(widgets.list().await.item(made.id()), &renamed);

    let versions = [
        widgets.announced::<WidgetCreated>().await[0].0,
        widgets.announced::<WidgetRenamed>().await[0].0,
    ];
    assert_eq!(versions, [1, 2].map(Version::from_i64));
}
