-- Events are delivered by topic (spec §4): an event type lost its version
-- (`users.user_deleted`), which moved to its topic (`users.user.v1`), and its
-- envelope names the topic and its key. Nothing from before can be delivered
-- by a worker of this build, so the release drains messaging first: run the
-- old build's worker until nothing is left to deliver, then migrate.
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
-- Registered under their module, with the old types; `migrate` registers them
-- again under their topics. Retired ones stay, so their names stay taken.
DELETE FROM messaging.subscription WHERE retired_at IS NULL;
