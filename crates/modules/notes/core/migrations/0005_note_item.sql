-- A note's checklist. Items are children of their note (an aggregate's child,
-- not a root): saved with it under its version, gone with it.
CREATE TABLE notes.note_item (
    note_id  uuid    NOT NULL REFERENCES notes.note (id) ON DELETE CASCADE,
    id       uuid    NOT NULL,
    text     text    NOT NULL,
    -- The order items were added in.
    position integer NOT NULL,
    PRIMARY KEY (note_id, id),
    UNIQUE (note_id, position)
);
