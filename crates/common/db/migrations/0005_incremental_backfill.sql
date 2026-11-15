-- An `earliest` subscription is backfilled by the worker, a chunk at a time,
-- rather than by `migrate` in one transaction (ADR 0035).

-- Numbers relay batches in the order they commit: one relay runs at a time and
-- holds its lock until it commits. `seq` can't serve, as it isn't commit order.
CREATE SEQUENCE messaging.relay_batch;

-- Which relay batch dispatched the event. Events dispatched before batches were
-- numbered belong to batch 0, before every numbered one.
ALTER TABLE messaging.outbox ADD COLUMN relay_batch bigint;
UPDATE messaging.outbox SET relay_batch = 0 WHERE dispatched_at IS NOT NULL;

-- One row per `earliest` subscription, written by `migrate` when it first
-- registers it. Its events are those of its topic and type relayed in a batch
-- up to `through_batch`, a set that only compaction shrinks; the relay delivers
-- every later one. The worker copies them into `messaging.job` in `seq` order,
-- moving `after_seq` past each chunk in the transaction that copies it.
CREATE TABLE messaging.backfill (
    queue         text        NOT NULL,
    event_type    text        NOT NULL,
    topic         text        NOT NULL,
    through_batch bigint      NOT NULL,
    after_seq     bigint      NOT NULL DEFAULT 0,
    registered_at timestamptz NOT NULL,
    completed_at  timestamptz,
    PRIMARY KEY (queue, event_type),
    FOREIGN KEY (queue, event_type) REFERENCES messaging.subscription (queue, event_type)
);

CREATE INDEX outbox_backfill_idx ON messaging.outbox (topic, event_type, seq);
