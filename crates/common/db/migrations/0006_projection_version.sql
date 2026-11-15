-- The version each projection (named by its queue) last applied of each key,
-- so `app_db::Projection` applies an event only when it is newer: a rerun dead
-- letter arrives out of order. A removed key keeps its row as a tombstone, so
-- an older event never brings its copy back (ADR 0036).
CREATE TABLE messaging.projection_version (
    queue   text    NOT NULL,
    key     text    NOT NULL,
    version bigint  NOT NULL,
    removed boolean NOT NULL,
    PRIMARY KEY (queue, key)
);
