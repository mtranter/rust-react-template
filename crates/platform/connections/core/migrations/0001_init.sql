-- Runs with search_path set to "connections".
CREATE SCHEMA IF NOT EXISTS connections;

-- One row per Connection a module has declared (ADR 0050), kept with its values once no
-- module declares it (`declared = false`), so declaring it again finds them.
CREATE TABLE connections.connection (
    key         text    PRIMARY KEY,
    label       text    NOT NULL,
    -- `connections_api::Declaration` as JSON.
    declaration jsonb   NOT NULL,
    declared    boolean NOT NULL,
    url         text,
    fields      jsonb   NOT NULL DEFAULT '{}',
    version     bigint  NOT NULL DEFAULT 1
);

-- A Connection's secrets, sealed with AES-256-GCM under a data key of their own, which
-- `key_version`'s master key wraps (envelope encryption): never stored in the clear.
CREATE TABLE connections.secret (
    connection_key text        NOT NULL REFERENCES connections.connection (key) ON DELETE CASCADE,
    name           text        NOT NULL,
    ciphertext     bytea       NOT NULL,
    wrapped_key    bytea       NOT NULL,
    key_version    text        NOT NULL,
    set_at         timestamptz NOT NULL,
    set_by         text        NOT NULL,
    PRIMARY KEY (connection_key, name)
);
