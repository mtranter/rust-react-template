-- Optimistic-concurrency version for the account aggregate. Runs with
-- search_path set to "users". Every write bumps it; updates match on it (see the
-- db layer's `save`), so a display-name change from a stale tab is refused
-- rather than silently clobbering a change the same user made elsewhere.
ALTER TABLE users.account ADD COLUMN version bigint NOT NULL DEFAULT 1;
