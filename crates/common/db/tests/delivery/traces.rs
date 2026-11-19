use crate::support::{Messaging, PingTopic, ping, run};
use app_events::PublishedTopic;
use app_persistence::Handlers;
use app_web::testing::Traces;
use chrono::TimeDelta;
use opentelemetry::trace::SpanId;
use opentelemetry_sdk::trace::SpanData;
use sqlx::PgPool;
use tracing::Instrument;

const QUEUE: &[&str] = &["tests.first"];

fn jobs(traces: &Traces) -> Vec<SpanData> {
    traces
        .spans()
        .into_iter()
        .filter(|span| span.name == "job")
        .collect()
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn an_event_is_handled_in_the_trace_it_was_published_in_each_retry_a_sibling(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    let worker = messaging.worker(QUEUE);
    let traces = Traces::install();
    messaging.fails("alice", 1, 1);

    messaging
        .publish(vec![ping("alice", 1)])
        .instrument(tracing::info_span!("publishing"))
        .await
        .unwrap();
    run(&worker).await;
    messaging.later(TimeDelta::minutes(1));
    run(&worker).await;

    let publishing = traces.span("publishing");
    let runs = jobs(&traces);
    assert_eq!(runs.len(), 2);
    for (attempt, job) in (1..).zip(&runs) {
        assert_eq!(
            job.span_context.trace_id(),
            publishing.span_context.trace_id()
        );
        assert_eq!(job.parent_span_id, publishing.span_context.span_id());
        assert_eq!(
            Traces::attribute(job, "attempt").unwrap(),
            attempt.to_string()
        );
        assert_eq!(
            Traces::attribute(job, "messaging.destination.name").unwrap(),
            "tests.first"
        );
        assert_eq!(
            Traces::attribute(job, "event_type").unwrap(),
            "tests.pinged"
        );
        assert_eq!(Traces::attribute(job, "key").unwrap(), "alice");
        assert_eq!(Traces::attribute(job, "version").unwrap(), "1");
    }
    assert_eq!(messaging.log.handled(), ["tests.first: pinged alice v1"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn an_event_published_outside_any_trace_is_handled_in_a_new_one(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    let worker = messaging.worker(QUEUE);
    let traces = Traces::install();

    messaging.ping("alice").await;
    let envelope: serde_json::Value = sqlx::query_scalar("SELECT payload FROM messaging.outbox")
        .fetch_one(&messaging.pool)
        .await
        .unwrap();
    run(&worker).await;

    assert!(envelope.get("traceparent").is_none());
    assert!(envelope.get("tracestate").is_none());
    let [job] = jobs(&traces).try_into().unwrap();
    assert_eq!(job.parent_span_id, SpanId::INVALID);
    assert!(job.span_context.is_valid());
    assert_eq!(messaging.log.handled(), ["tests.first: pinged alice v1"]);
}

#[app_test_db::test(crate::support::TEMPLATE)]
async fn the_worker_traces_each_relay_batch_backfill_chunk_and_compaction(pool: PgPool) {
    let messaging = Messaging::new(pool).await;
    messaging.migrate(QUEUE).await;
    messaging.ping("alice").await;
    run(&messaging.worker(QUEUE)).await;
    let copying = Handlers::new()
        .on_from_earliest::<PingTopic>("tests.copy", messaging.recorder("tests.copy"));
    messaging.register(&copying).await.unwrap();
    messaging
        .register_topics(&[PublishedTopic::of::<PingTopic>()])
        .await;
    let traces = Traces::install();

    messaging.ping("bob").await;
    let worker = messaging.worker_of(copying);
    run(&worker).await;
    worker.compact().await.unwrap();

    let relay = traces.span("relay_batch");
    assert_eq!(Traces::attribute(&relay, "events").unwrap(), "1");
    let chunks: Vec<_> = traces
        .spans()
        .into_iter()
        .filter(|span| span.name == "backfill_chunk")
        .map(|span| {
            (
                Traces::attribute(&span, "queue").unwrap(),
                Traces::attribute(&span, "events").unwrap(),
            )
        })
        .collect();
    // The second copies nothing and completes the backfill.
    assert_eq!(
        chunks,
        [
            ("tests.copy".into(), "1".into()),
            ("tests.copy".into(), "0".into())
        ]
    );
    let compaction = traces.span("compaction");
    assert_eq!(Traces::attribute(&compaction, "deleted").unwrap(), "0");
}
