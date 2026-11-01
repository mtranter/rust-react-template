-- Which queues the relay fans each event out to (spec §4). Written by
-- `migrate` (`app_db::register_subscriptions`), before a rollout, never by a
-- worker; the relay joins it, so every build relays the same way. A queue is
-- bound to one topic forever, and a retired one's name is never reused.
--
-- Retiring a subscription is a migration of two statements: stop relaying to
-- it, and dead-letter what it hasn't run yet, where it stays visible:
--
--   UPDATE messaging.subscription SET retired_at = now() WHERE queue = '<queue>';
--   UPDATE messaging.job SET dead_at = now(), last_error = 'subscription retired'
--   WHERE queue = '<queue>' AND dead_at IS NULL;
CREATE TABLE messaging.subscription (
    queue         text        NOT NULL,
    event_type    text        NOT NULL,
    topic         text        NOT NULL,
    registered_at timestamptz NOT NULL,
    retired_at    timestamptz,
    PRIMARY KEY (queue, event_type)
);
