use crate::support::{Messaging, ping};
use serde_json::Value;
use sqlx::PgPool;

#[sqlx::test(migrations = false)]
async fn an_event_carries_its_roots_version_on_its_row_and_in_its_envelope(pool: PgPool) {
    let messaging = Messaging::new(pool).await;

    messaging.publish(vec![ping("alice", 3)]).await.unwrap();

    let (topic, key, version, envelope): (String, String, i64, Value) =
        sqlx::query_as("SELECT topic, key, version, payload FROM messaging.outbox")
            .fetch_one(&messaging.pool)
            .await
            .unwrap();
    assert_eq!(
        (topic.as_str(), key.as_str(), version),
        ("tests.ping.v1", "alice", 3)
    );
    assert_eq!(envelope["topic"], "tests.ping.v1");
    assert_eq!(envelope["partitionkey"], "alice");
    assert_eq!(envelope["entityversion"], 3);
}

#[sqlx::test(migrations = false)]
async fn a_second_event_for_one_root_in_one_save_is_refused(pool: PgPool) {
    let messaging = Messaging::new(pool).await;

    let refused = messaging
        .publish(vec![ping("alice", 2), ping("alice", 2)])
        .await;

    assert!(refused.is_err());
    let published: i64 = sqlx::query_scalar("SELECT count(*) FROM messaging.outbox")
        .fetch_one(&messaging.pool)
        .await
        .unwrap();
    assert_eq!(published, 0);
}
