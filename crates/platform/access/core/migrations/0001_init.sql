-- Runs with search_path set to "access" (ADR 0021).
CREATE SCHEMA IF NOT EXISTS access;

-- Role → action, within a scope (ADR 0040). `scope` is `any` or one of the
-- resource's scopes, which can't be named `any`.
CREATE TABLE access.grant (
    id         uuid        PRIMARY KEY,
    role       text        NOT NULL,
    resource   text        NOT NULL,
    action     text        NOT NULL,
    scope      text        NOT NULL,
    -- The subject who granted it; NULL for a default grant, applied as `System`.
    granted_by text,
    granted_at timestamptz NOT NULL,
    version    bigint      NOT NULL DEFAULT 1,
    CONSTRAINT grant_once UNIQUE (role, resource, action, scope)
);

-- One row, bumped in the unit of work of every change to the grants: every
-- server instance polls it, and rebuilds its rule set when it has moved.
CREATE TABLE access.revision (
    id       boolean PRIMARY KEY DEFAULT true CHECK (id),
    revision bigint  NOT NULL
);
INSERT INTO access.revision (id, revision) VALUES (true, 0);

-- Each module's default grant `migrate` has applied, by its key, so a revoked
-- default isn't granted again.
CREATE TABLE access.applied_default (
    module     text        NOT NULL,
    role       text        NOT NULL,
    resource   text        NOT NULL,
    action     text        NOT NULL,
    scope      text        NOT NULL,
    applied_at timestamptz NOT NULL,
    PRIMARY KEY (module, role, resource, action, scope)
);
