-- Retiring a subscription is one call in a migration (ticket 0015):
--
--   SELECT messaging.retire('<queue>');
--
-- It supersedes the two-statement recipe in 0001_messaging.sql, which a relay
-- or a backfill chunk in flight could slip live jobs past.
--
-- plpgsql, not one SQL statement: under READ COMMITTED each statement here
-- takes a fresh snapshot, so the last sees what the first two waited for.
CREATE FUNCTION messaging.retire(retiring text) RETURNS void
LANGUAGE plpgsql AS $$
BEGIN
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
