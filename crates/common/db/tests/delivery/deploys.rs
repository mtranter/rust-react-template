use crate::support::{Job, Messaging, run};
use sqlx::PgPool;

#[sqlx::test(migrations = false)]
async fn an_old_worker_leaves_a_subscription_it_doesnt_serve_to_one_that_does(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    let new = messaging.worker(&["tests.old", "tests.new"]);
    let old = messaging.worker(&["tests.old"]);
    messaging.ping("alice").await;
    new.relay().await.expect("relay");

    run(&old).await;

    assert_eq!(messaging.log.handled(), ["tests.old: alice"]);
    assert_eq!(messaging.jobs().await, [Job::untouched("tests.new")]);

    run(&new).await;

    assert_eq!(
        messaging.log.handled(),
        ["tests.old: alice", "tests.new: alice"]
    );
    assert!(messaging.jobs().await.is_empty());
}
