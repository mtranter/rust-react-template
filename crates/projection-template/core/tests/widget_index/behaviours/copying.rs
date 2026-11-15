use crate::driver::{Refusal, WidgetIndex};
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn nothing_is_listed_until_the_copy_has_caught_up(pool: PgPool) {
    let index = WidgetIndex::added_after_the_seeds(pool).await;
    assert_eq!(index.all().await.err(), Some(Refusal::Unavailable));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn every_root_announced_before_it_existed_is_listed(pool: PgPool) {
    let index = WidgetIndex::seeded(pool).await;

    let mut listed = index.all().await.expect("caught up").ids();
    listed.sort();

    let announced = index.announced_keys().await;
    assert!(
        !announced.is_empty(),
        "the provider's seed announces its roots"
    );
    assert_eq!(listed, announced);
}
