-- Every topic a build publishes, with how long its history is kept. Written by
-- `migrate` (`app_db::register_topics`), so every build compacts alike during a
-- rolling deploy; the worker compacts only the topics registered here.
CREATE TABLE messaging.topic (
    name          text        PRIMARY KEY,
    retention     interval    NOT NULL,
    registered_at timestamptz NOT NULL
);

-- Compaction looks for relayed events past their topic's retention, then for a
-- later version of each one's key, by `outbox_event_idx`.
CREATE INDEX outbox_relayed_idx ON messaging.outbox (occurred_at) WHERE dispatched_at IS NOT NULL;
