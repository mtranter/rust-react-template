-- What the last Test connection found (ADR 0050), for the Connection's version it tested: once
-- the Connection changes, the row no longer matches and the Connection reads as untested.
-- Observed, not decided: kept beside the aggregate rather than in it, so a test bumps no
-- version and announces nothing, and an admin's open form doesn't go stale.
CREATE TABLE connections.health (
    connection_key text        PRIMARY KEY REFERENCES connections.connection (key) ON DELETE CASCADE,
    version        bigint      NOT NULL,
    ok             boolean     NOT NULL,
    error          text,
    tested_at      timestamptz NOT NULL
);
