# Lists sort by closed keys, through the one sanctioned runtime SQL

A list use case takes a `ListQuery<XSortKey>` (an optional page and a list of sorts) and returns `Listed<T>`, with a total only when paged. Limits default to 20 and are clamped to 100. `XSortKey` is a closed enum in the Application Layer, so no caller-supplied text reaches SQL: the DB Layer maps each key to a fixed column and builds the query with `QueryBuilder` and `app_db::push_list_clauses`, always ending the order in a unique tiebreak so pages are stable. It is the one sanctioned runtime SQL, because `query!` can't vary `ORDER BY`.

Over HTTP the sort is a poem `Enum` whose values are the response's camelCase field names, `-`-prefixed for descending, so the Generated Client's TypeScript union names exactly the sortable columns, and the frontend's data table is typed against it. The handler takes `limit`, `offset` and `sort` as separate parameters, since poem-openapi has no parameter structs, and returns `ListResult<XResponse>`.

## Considered options

- **Free-text sort fields.** Caller text would reach SQL, and the client couldn't know what is sortable.
- **One `query!` per sort order.** Combinatorial.

## Consequences

- An unsortable list omits `sort`; an unpaged one omits `limit`/`offset` and uses `ListQuery::all()`.
- A list a write Module can't serve from its own schema belongs in a Read Module (ADR 0009).
