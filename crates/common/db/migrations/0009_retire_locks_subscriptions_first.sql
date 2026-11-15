-- `messaging.retire` takes its locks in the order registering subscriptions
-- does (`register_subscriptions`): the subscription table, then the relay's
-- lock. Taking the relay's lock first (migration 0008), it deadlocked with a
-- `migrate` registering an `earliest` subscription at the same time, each
-- holding the lock the other waited for (ticket 0023).
CREATE OR REPLACE FUNCTION messaging.retire(retiring text) RETURNS void
LANGUAGE plpgsql AS $$
BEGIN
    LOCK TABLE messaging.subscription IN SHARE ROW EXCLUSIVE MODE;
    -- The relay's lock (`RELAY_LOCK` in worker.rs): waits for a relay in
    -- flight to commit its jobs, and holds off the next until this commits.
    PERFORM pg_advisory_xact_lock(491327873401);
    UPDATE messaging.subscription SET retired_at = now()
    WHERE queue = retiring AND retired_at IS NULL;
    -- Waits for a chunk being copied to commit; a worker that read the row
    -- before this commits re-checks `completed_at` when it locks it, and skips it.
    UPDATE messaging.backfill SET completed_at = now()
    WHERE queue = retiring AND completed_at IS NULL;
    UPDATE messaging.job SET dead_at = now(), last_error = 'subscription retired'
    WHERE queue = retiring AND dead_at IS NULL;
END
$$;
