-- Where each subscription started: `latest`, receiving only events relayed after
-- `migrate` registered it, or `earliest`, backfilled then with every event its
-- topic's outbox held. Recorded for visibility: a registered subscription's start
-- never changes.
ALTER TABLE messaging.subscription ADD COLUMN starts text NOT NULL DEFAULT 'latest'
    CHECK (starts IN ('latest', 'earliest'));

-- Each outbox event paired with every live subscription to its topic and type,
-- as the job that delivers it: the relay and an `earliest` backfill both copy
-- from here, each choosing its own events.
CREATE VIEW messaging.delivery AS
SELECT o.seq, o.dispatched_at, s.queue, o.event_type, o.key, o.version, o.payload
FROM messaging.outbox o
JOIN messaging.subscription s
  ON s.topic = o.topic AND s.event_type = o.event_type
 AND s.retired_at IS NULL;
