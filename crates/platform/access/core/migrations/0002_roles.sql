-- Runs with search_path set to "access" (ADR 0021).

-- A role grants are made to (and, later, subjects are assigned). Its name is its
-- id and never changes. `admin` and `user` are built in and can't be deleted.
CREATE TABLE access.role (
    name        text        PRIMARY KEY,
    description text        NOT NULL,
    -- The subject who created it; NULL for a built-in role, or one `migrate`
    -- created for a module's default grants.
    created_by  text,
    created_at  timestamptz NOT NULL,
    version     bigint      NOT NULL DEFAULT 1
);

INSERT INTO access.role (name, description, created_at) VALUES
    ('admin', 'Administers the app: always reaches access, whatever is granted.', now()),
    ('user', 'Every signed-in user.', now());

-- Grants made before roles were records name roles that must now exist.
INSERT INTO access.role (name, description, created_at)
SELECT DISTINCT role, '', now() FROM access.grant
ON CONFLICT (name) DO NOTHING;

-- A grant names a role that exists, and a role a grant names can't be deleted.
ALTER TABLE access.grant
    ADD CONSTRAINT grant_role_exists FOREIGN KEY (role) REFERENCES access.role (name);

-- The version each deleted role's name reached (its deletion's), so a role
-- created again under that name continues from it: events are keyed by the
-- name, and ordered per key by version.
CREATE TABLE access.deleted_role (
    name    text   PRIMARY KEY,
    version bigint NOT NULL
);
