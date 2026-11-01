-- The outbox and job queue (spec §4): infrastructure every module writes through
-- `app_db`, not any module's data. Runs with search_path set to "messaging",
-- before every module's migrations.
CREATE SCHEMA IF NOT EXISTS messaging;

-- Written in the same transaction as the change it announces; the worker's relay
-- copies each event into one job per subscriber and marks it dispatched.
CREATE TABLE messaging.outbox (
    seq           bigserial   PRIMARY KEY,
    event_type    text        NOT NULL,
    payload       jsonb       NOT NULL,
    occurred_at   timestamptz NOT NULL,
    dispatched_at timestamptz
);

CREATE INDEX outbox_undispatched_idx ON messaging.outbox (seq) WHERE dispatched_at IS NULL;

-- One row per job or event delivery. A claim pushes `run_at` forward by a lease,
-- so a job whose worker died runs again; success deletes the row; a failure
-- records `last_error` and backs off, until `dead_at` dead-letters it.
CREATE TABLE messaging.job (
    id           bigserial   PRIMARY KEY,
    queue        text        NOT NULL,
    payload      jsonb       NOT NULL,
    run_at       timestamptz NOT NULL,
    attempts     integer     NOT NULL DEFAULT 0,
    max_attempts integer     NOT NULL,
    last_error   text,
    dead_at      timestamptz,
    created_at   timestamptz NOT NULL
);

CREATE INDEX job_due_idx ON messaging.job (run_at) WHERE dead_at IS NULL;
