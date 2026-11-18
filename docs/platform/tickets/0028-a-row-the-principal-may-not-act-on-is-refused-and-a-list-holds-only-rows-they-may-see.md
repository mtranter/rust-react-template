---
id: 0028
name: A row the principal may not act on is refused, and a list holds only the rows they may see, filtered in its SQL
status: in-review
priority: P1
size: M
blocked_by: [0026, 0027]
blocks: []
source: conversation, 2026-11-17 (an authz pattern at module, action and row level; row level last)
---

# A row the principal may not act on is refused, and a list holds only the rows they may see, filtered in its SQL

## Description

ADR 0040's row level. After ticket 0027, a `Where` plan is let through. This ticket adds two things:

- **One row:** checked once it is loaded.
- **A list:** the plan compiled into its query, so paging and totals count only what the caller may see.

`notes` is the reference: only a note's author (or an `admin`) sees, edits or archives it.

## Evidence (as of 2026-11-17)

- Lists are built with `QueryBuilder` and `app_db::push_list_clauses` (`crates/common/db/src/listing.rs`), the one sanctioned runtime SQL (ADR 0018). It appends only `ORDER BY` and `LIMIT`/`OFFSET`, so there is no `WHERE` to join.
- `notes`' list is `SELECT … FROM notes.note` with no filter (`crates/reference/notes/core/src/db/repository.rs`). Its total is counted separately for a paged query.
- `note-search` serves notes from its own projection. It is a different resource, so it is outside this ticket unless it adopts a policy (see Open questions).

## Proposed approach

1. **Columns.** `app_db::Columns` for a resource: each attribute in `Resource::ATTRS` maps to a column or a module-authored SQL expression. A missing mapping is caught by a test the trait provides, run from each Module's DB tests.
2. **SQL.** `app_db::push_scope(sql, &filter, columns)`:
   - appends `WHERE` (or `AND`) and the filter, every value bound and never interpolated;
   - `In` becomes `= ANY($n)`;
   - `null` comparisons become `IS [NOT] NULL`;
   - `True` appends nothing and `False` appends `FALSE`.

   It runs before `push_list_clauses` and in the total's query, so both count the same rows.
3. **Repository.** `list(&ListQuery<K>, &Scope)`, where `Scope` is the plan's filter, or everything (`Allowed`). A `Denied` plan never reaches the repository: the service has already refused it (ticket 0027).
4. **One row.** After loading, the service checks `filter.matches(&row)`. Refused, it is `404` when the principal's `view` plan doesn't match the row either, and `403` otherwise (ADR 0040). The check happens before any domain action, inside the unit of work.
5. **Agreement test.** A helper in `app-authz`'s test support, used by `notes`' DB tests:
   - seed rows with varied attributes;
   - for each of several principals, check that the rows the scoped list returns are exactly the rows `filter.matches` accepts.

   This keeps the SQL and in-memory paths from drifting.
6. **Docs.** `crates/AGENTS.md`: a rule (lists take a `Scope`; never filter after the query) and the reference row pointing at `notes`' repository.

## Acceptance criteria

- [ ] Another user's note:
  - [ ] `get` is `404`;
  - [ ] `edit` and `archive` are `404` and change nothing.
- [ ] An `admin` can see and change any note.
- [ ] `list_notes` returns only the caller's notes, and its total and pages count only those.
- [ ] A rule allowing `view` on `R.attr.visibility == "public"` (in a test policy, not the reference) makes a public note `403` to edit, not `404`.
- [ ] The agreement test passes for `notes`, and fails when the column mapping is deliberately wrong.
- [ ] No value from a principal or policy is interpolated into SQL (every value is bound; checked by a test on the built SQL).
- [ ] The journeys from ticket 0027 are updated: a non-author is now refused. `make check` passes.

## Open questions

- Whether `note-search` declares a policy for its own resource, so search results are scoped as the list is. Its projection would need `authorId`, which it may already copy.
