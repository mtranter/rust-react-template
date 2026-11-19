-- Schedules (ADR 0043): a module's recurring work, declared in code and
-- registered here by `migrate` (`app_db::register_schedules`), never by a
-- worker. Each pass of a worker's poll loop turns each schedule whose
-- `next_at` has passed into one job, then moves `next_at` past its clock.
-- A run is a deferred job keyed by the schedule's name (queue, event type and
-- key are all the name), so `job_deferred_key_idx` lets a schedule have only
-- one run pending or running at once: a slot due meanwhile is skipped.
CREATE TABLE messaging.schedule (
    name          text        PRIMARY KEY,
    cron          text        NOT NULL,
    next_at       timestamptz NOT NULL,
    registered_at timestamptz NOT NULL
);

CREATE INDEX schedule_due_idx ON messaging.schedule (next_at);
