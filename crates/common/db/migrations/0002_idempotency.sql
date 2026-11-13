-- Idempotency records: the answer to each unsafe request sent with an
-- `Idempotency-Key`, per caller and key, replayed to a retry. Infrastructure
-- like the outbox, written by `app_db::PgIdempotencyRecords`, not any module's
-- data. A record older than 24 hours is expired: the next claim of its key
-- replaces it.
CREATE SCHEMA IF NOT EXISTS idempotency;

CREATE TABLE idempotency.request (
    -- The bearer token's subject, or '' for a caller who sent none.
    caller        text        NOT NULL,
    key           text        NOT NULL,
    -- SHA-256 of the method, target, If-Match and body, so the same key sent
    -- with a different request is told apart.
    request_hash  bytea       NOT NULL,
    claimed_at    timestamptz NOT NULL,
    -- Null while the request is in flight.
    status        smallint,
    headers       jsonb,
    body          bytea,
    PRIMARY KEY (caller, key)
);

-- No job removes expired records: each claim deletes them instead, and this
-- keeps that delete cheap.
CREATE INDEX request_claimed_at ON idempotency.request (claimed_at);
