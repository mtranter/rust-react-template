-- A copy of widgets' own roots in a shape for searching them, kept from its
-- own topic from the earliest (ADR 0036). Rows are written only through
-- `app_db::Projecting`, which keeps their versions. Keep a copy like this only
-- for a read the aggregate's tables can't serve well, or point it at another
-- module's topic to search by that module's columns; otherwise delete it.
CREATE TABLE widgets.widget_search (
    id   uuid PRIMARY KEY,
    name text NOT NULL
);

CREATE INDEX widget_search_name_idx ON widgets.widget_search (lower(name) text_pattern_ops, id);
