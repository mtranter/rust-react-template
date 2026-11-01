-- Events are delivered by topic (spec §4): an event type lost its version
-- (`users.user_deleted`), which moved to its topic (`users.user.v1`), and every
-- event names its topic, its key (the root's id) and the root's version. Rows
-- from before have none of those, and can't be backfilled, so the release
-- drains messaging first: run the old build's worker until nothing is left to
-- deliver, then migrate. The dispatched rows from before are deleted.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM messaging.outbox WHERE dispatched_at IS NULL)
       OR EXISTS (SELECT 1 FROM messaging.job WHERE dead_at IS NULL) THEN
        RAISE EXCEPTION 'messaging holds events from before topics still to deliver: '
            'run the previous build''s worker until messaging.outbox is dispatched '
            'and messaging.job holds only dead letters, then migrate again';
    END IF;
END
$$;

DELETE FROM messaging.outbox;

ALTER TABLE messaging.outbox
    ADD COLUMN topic   text   NOT NULL,
    ADD COLUMN key     text   NOT NULL,
    ADD COLUMN version bigint NOT NULL;

-- One event per root per save: a second for the same save fails it.
CREATE UNIQUE INDEX outbox_event_idx ON messaging.outbox (topic, key, version);
-- Registered under their module, with the old types; `migrate` registers them
-- again under their topics. Retired ones stay, so their names stay taken.
DELETE FROM messaging.subscription WHERE retired_at IS NULL;

-- A job carries its event's key and version, so a claim waits for the earlier
-- versions of its key in its queue (spec §4). The only jobs left from before
-- are dead letters, which no longer decode; they get their subject as key and
-- version 0, so they block nothing.
ALTER TABLE messaging.job
    ADD COLUMN key     text,
    ADD COLUMN version bigint;
UPDATE messaging.job SET key = payload->>'subject', version = 0;
ALTER TABLE messaging.job
    ALTER COLUMN key SET NOT NULL,
    ALTER COLUMN version SET NOT NULL;

CREATE INDEX job_key_idx ON messaging.job (queue, key, version) WHERE dead_at IS NULL;
