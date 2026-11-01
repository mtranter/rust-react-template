-- A worker claims only the `(queue, event_type)` pairs its build can decode, so
-- during a rolling deploy an old worker leaves a new subscription's jobs alone
-- instead of failing them (spec §4). Every job so far was relayed from an
-- envelope that names its type.
ALTER TABLE messaging.job ADD COLUMN event_type text;
UPDATE messaging.job SET event_type = payload->>'type';
ALTER TABLE messaging.job ALTER COLUMN event_type SET NOT NULL;

-- The claim looks up each pair's first due job. Ordered by `run_at` alone, it
-- would scan past every due job no running worker serves (a new subscription's,
-- mid-deploy) to find one it does.
DROP INDEX messaging.job_due_idx;
CREATE INDEX job_due_idx ON messaging.job (queue, event_type, run_at, id) WHERE dead_at IS NULL;
