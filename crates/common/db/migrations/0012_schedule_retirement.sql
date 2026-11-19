-- A schedule no build declares any more is retired by `migrate`
-- (`app_db::register_schedules`), never deleted: a worker never turns a
-- retired schedule's slot into a run, and declaring its name again
-- reactivates it, due at its next slot.
ALTER TABLE messaging.schedule ADD COLUMN retired_at timestamptz;

DROP INDEX messaging.schedule_due_idx;
CREATE INDEX schedule_due_idx ON messaging.schedule (next_at) WHERE retired_at IS NULL;
