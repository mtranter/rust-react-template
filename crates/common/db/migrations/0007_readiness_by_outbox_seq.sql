-- Readiness is judged by outbox `seq` rather than by clocks (ticket 0013).

-- The outbox event each job delivers. Jobs from before this column count as 0,
-- at or below every boundary, so they are waited for.
ALTER TABLE messaging.job ADD COLUMN outbox_seq bigint NOT NULL DEFAULT 0;
ALTER TABLE messaging.job ALTER COLUMN outbox_seq DROP DEFAULT;

-- The outbox's highest `seq` when `migrate` registered the subscription: every
-- event its topic then held, relayed or not, is at or below it. Backfills
-- registered before this column wait for everything the outbox now holds.
ALTER TABLE messaging.backfill ADD COLUMN through_seq bigint NOT NULL DEFAULT 0;
UPDATE messaging.backfill
SET through_seq = (SELECT coalesce(max(seq), 0) FROM messaging.outbox);
ALTER TABLE messaging.backfill ALTER COLUMN through_seq DROP DEFAULT;

CREATE INDEX job_outbox_seq_idx ON messaging.job (queue, event_type, outbox_seq)
    WHERE dead_at IS NULL;
