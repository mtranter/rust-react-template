-- Runs with search_path set to "access" (ADR 0021).

-- Who changed access and when: one row per event of the grant, role and
-- assignment topics, filled from the earliest each holds (ADR 0036), so
-- history older than a topic's retention survives only as each key's latest
-- event. Append-only: an event's id makes a redelivery write nothing.
CREATE TABLE access.audit_entry (
    id          uuid        PRIMARY KEY,
    -- The order entries were copied in: ties on `occurred_at` (one clock
    -- reading for several changes) break by it.
    seq         bigserial   NOT NULL,
    occurred_at timestamptz NOT NULL,
    -- `accessGranted`, `grantRevoked`, `roleCreated`, `roleDeleted`,
    -- `roleAssigned` or `roleUnassigned`.
    change      text        NOT NULL,
    -- The subject who made the change; NULL for `System` (`migrate`, a seed).
    actor       text,
    role        text        NOT NULL,
    -- The assignment's subject; NULL for a grant's or a role's change.
    subject     text,
    -- The grant's; NULL for a role's or an assignment's change.
    resource    text,
    action      text,
    scope       text
);

CREATE INDEX audit_entry_occurred_at_idx ON access.audit_entry (occurred_at DESC, seq DESC);
CREATE INDEX audit_entry_actor_idx ON access.audit_entry (actor);
CREATE INDEX audit_entry_role_idx ON access.audit_entry (role);
CREATE INDEX audit_entry_subject_idx ON access.audit_entry (subject);
