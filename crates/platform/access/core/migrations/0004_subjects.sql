-- Runs with search_path set to "access" (ADR 0021).

-- Each subject (the token's `sub`) seen signing in, so admins can pick whom to
-- assign roles from a list. A record of sightings, not an aggregate: it is
-- upserted, unversioned and announces nothing.
CREATE TABLE access.subject (
    subject    text        PRIMARY KEY,
    -- The token's `name` and `email` claims, as the latest token carrying them said.
    name       text,
    email      text,
    first_seen timestamptz NOT NULL,
    -- Accurate to within an hour: a sighting sooner after the last writes nothing.
    last_seen  timestamptz NOT NULL
);
