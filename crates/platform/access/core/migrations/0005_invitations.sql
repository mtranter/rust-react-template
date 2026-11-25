-- Runs with search_path set to "access" (ADR 0021).

-- An invitee is a subject before their first sign-in, with the name and email
-- the invitation gave, and no sightings.
ALTER TABLE access.subject ALTER COLUMN first_seen DROP NOT NULL;
ALTER TABLE access.subject ALTER COLUMN last_seen DROP NOT NULL;

-- Someone an admin had the identity provider create, holding `roles` from
-- then on (as assignments), until revoked before they sign in.
CREATE TABLE access.invitation (
    id         uuid        PRIMARY KEY,
    email      text        NOT NULL,
    name       text        NOT NULL,
    -- The identity provider's `sub` for the invitee.
    subject    text        NOT NULL,
    roles      text[]      NOT NULL,
    -- The subject who invited them; NULL for `System`.
    invited_by text,
    invited_at timestamptz NOT NULL,
    status     text        NOT NULL,
    version    bigint      NOT NULL DEFAULT 1,
    CONSTRAINT invitation_status CHECK (status IN ('pending', 'accepted', 'revoked'))
);

CREATE UNIQUE INDEX invitation_pending_email ON access.invitation (email)
    WHERE status = 'pending';
CREATE INDEX invitation_pending_subject ON access.invitation (subject)
    WHERE status = 'pending';
CREATE INDEX invitation_invited_at ON access.invitation (invited_at DESC, id);
