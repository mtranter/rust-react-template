-- Deferred jobs (ADR 0043): a module's commands to itself, written into the
-- unit of work that defers them (`app_db::PgDeferredJobs`) and run by the
-- worker like any delivery. Their queue and `event_type` are the job's type;
-- they have no outbox event, so `outbox_seq` is 0.
ALTER TABLE messaging.job ADD COLUMN deferred boolean NOT NULL DEFAULT false;

-- One pending job per type and key: deferring again replaces it (bumping its
-- `version`, so a run already under way can't complete it), cancelling
-- deletes it. A dead letter steps aside for the next.
CREATE UNIQUE INDEX job_deferred_key_idx ON messaging.job (queue, key)
    WHERE deferred AND dead_at IS NULL;
