-- The outbox and job queue (spec §4): infrastructure every module writes through
-- `app_db`, not any module's data. Runs with search_path set to "messaging",
-- before every module's migrations.
CREATE SCHEMA IF NOT EXISTS messaging;

-- Written in the same transaction as the change it announces; the worker's relay
-- copies each event into one job per subscription and marks it dispatched.
CREATE TABLE messaging.outbox (
    seq           bigserial   PRIMARY KEY,
    topic         text        NOT NULL,
    event_type    text        NOT NULL,
    key           text        NOT NULL,
    version       bigint      NOT NULL,
    payload       jsonb       NOT NULL,
    occurred_at   timestamptz NOT NULL,
    dispatched_at timestamptz
);

CREATE INDEX outbox_undispatched_idx ON messaging.outbox (seq) WHERE dispatched_at IS NULL;
-- One event per root per save: a second for the same save fails it.
CREATE UNIQUE INDEX outbox_event_idx ON messaging.outbox (topic, key, version);

-- Which queues the relay fans each event out to. Written by `migrate`
-- (`app_db::register_subscriptions`), before a rollout, never by a worker; the
-- relay joins it, so every build relays the same way. A queue is bound to one
-- topic forever, and a retired one's name is never reused.
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

-- One row per delivery of an event to a queue. A claim pushes `run_at` forward
-- by a lease, so a job whose worker died runs again; success deletes the row;
-- a failure records `last_error` and backs off, until `dead_at` dead-letters it.
CREATE TABLE messaging.job (
    id           bigserial   PRIMARY KEY,
    queue        text        NOT NULL,
    event_type   text        NOT NULL,
    key          text        NOT NULL,
    version      bigint      NOT NULL,
    payload      jsonb       NOT NULL,
    run_at       timestamptz NOT NULL,
    attempts     integer     NOT NULL DEFAULT 0,
    max_attempts integer     NOT NULL,
    last_error   text,
    dead_at      timestamptz,
    created_at   timestamptz NOT NULL
);

-- A worker claims only the `(queue, event_type)` pairs its build can decode, so
-- during a rolling deploy an old worker leaves a new subscription's jobs alone.
-- The claim looks up each pair's first due job; ordered by `run_at` alone, it
-- would scan past every due job no running worker serves to find one it does.
CREATE INDEX job_due_idx ON messaging.job (queue, event_type, run_at, id) WHERE dead_at IS NULL;
-- A claim waits for the earlier versions of its key in its queue.
CREATE INDEX job_key_idx ON messaging.job (queue, key, version) WHERE dead_at IS NULL;
