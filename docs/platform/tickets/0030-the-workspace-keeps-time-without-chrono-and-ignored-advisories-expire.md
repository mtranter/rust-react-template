---
id: 0030
name: "The workspace keeps time without `chrono`, and every ignored advisory carries an expiry"
status: needs-triage
priority: P2
size: M
blocked_by: []
blocks: []
source: "`make check` failing on RUSTSEC-2026-0543, 2026-11-17"
---

# The workspace keeps time without `chrono`, and every ignored advisory carries an expiry

## Description

RUSTSEC-2026-0543 reports `chrono` unmaintained, every version, with no safe upgrade; the advisory recommends `jiff` or `time`. It failed `make check`'s `deny` step, so it is ignored in `deny.toml` with a "revisit by 2027-03-31" comment. That date is only a comment: nothing enforces it. Two pieces of work:

1. **Move off `chrono`** to `time` or `jiff`, wherever it is used, and drop the ignore.
2. **Make ignores expire.** cargo-deny 0.21.0 (2026-11-16) added `expiry` to an `[advisories] ignore` entry (`{ id, reason, expiry }`, a date or an ISO-8601 duration), and a default `ignore-expiry`. Once it expires, the advisory is reported again with an `advisory-ignore-expired` diagnostic. We pin 0.20.2 (ADR 0027), which lacks it.

## Evidence (as of 2026-11-17)

- `chrono = { version = "0.4", … features = ["clock", "serde"] }` is a workspace dependency, and sqlx's `chrono` feature is on (root `Cargo.toml`). `cargo deny` traces it through `app-db` to every Module.
- The `deny.toml` ignores are plain ids with comments: `RUSTSEC-2023-0071`, `2026-0194`, `2026-0195`, `2026-0387`, `2024-0449`, `2026-0440` and `2026-0543`.
- The repo-local cargo-deny is 0.20.2 (`cargo bin cargo-deny --version`).

## Proposed approach

1. **Choose `time` or `jiff`.**
   - Check which of sqlx 0.9's features each has, and its Postgres `timestamptz` mapping.
   - Check that poem-openapi 5.1 has a `Type` for it in the spec, for timestamps in responses.
   - Check its serde format, which must stay identical, since events already in the Outbox were serialized with `chrono` (ADR 0012, 0033).
   - Prefer the one poem-openapi supports without a wrapper.
2. **Swap it through:**
   - the `Clock` (ADR 0020) and `app-db`;
   - every Module's domain timestamps and their `query!` mappings (`make prepare`);
   - Seeds' `seed_time`;
   - the module template.
3. **Prove wire compatibility.** A test reads an event serialized by the `chrono` build and gets the same instant. The generated client's timestamp types are unchanged (`make check-spec`).
4. **Bump cargo-deny to 0.21.x** once it has had a couple of weeks out. Turn every ignore into `{ id, reason, expiry }`, and set a default `ignore-expiry`, so an ignore can't outlive its reason silently. Drop `RUSTSEC-2026-0543`.

## Acceptance criteria

- [ ] `cargo tree -i chrono` finds nothing.
- [ ] Every timestamp in the spec and the generated client is unchanged, and an Outbox event written before the change still deserializes to the same instant.
- [ ] cargo-deny is 0.21.x, and every `ignore` entry has a `reason` and an `expiry`.
- [ ] `make check` passes.
