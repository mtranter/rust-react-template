-- Runs with search_path set to "access" (ADR 0021).

-- A role assigned to a subject (the token's `sub`): a user holds `user`, which is
-- never stored, and the roles assigned to them here.
CREATE TABLE access.assignment (
    subject     text        NOT NULL,
    role        text        NOT NULL,
    -- The subject who assigned it; NULL for one a seed assigned as `System`.
    assigned_by text,
    assigned_at timestamptz NOT NULL,
    version     bigint      NOT NULL DEFAULT 1,
    PRIMARY KEY (subject, role),
    -- An assignment names a role that exists, and a role it names can't be deleted.
    CONSTRAINT assignment_role_exists FOREIGN KEY (role) REFERENCES access.role (name),
    CONSTRAINT assignment_not_user CHECK (role <> 'user')
);

CREATE INDEX assignment_role ON access.assignment (role);

-- The version each removed assignment reached (its removal's), so one assigned
-- again continues from it: events are keyed by subject and role, and ordered
-- per key by version.
CREATE TABLE access.unassigned (
    subject text   NOT NULL,
    role    text   NOT NULL,
    version bigint NOT NULL,
    PRIMARY KEY (subject, role)
);
