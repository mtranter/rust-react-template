---
id: 0026
name: Policy files and grants compile, checked against their resources' Rust types, into one swappable rule set that plans a principal's action as allowed, denied or a filter
status: in-progress
priority: P1
size: L
blocked_by: []
blocks: [0027, 0028, 0029]
source: conversation, 2026-11-17 (an authz pattern at module, action and row level; Cerbos-inspired policies chosen over gatehouse for compiling to SQL; grants editable by admins without a deploy)
---

# Policy files and grants compile, checked against their resources' Rust types, into one swappable rule set that plans a principal's action as allowed, denied or a filter

## Description

ADR 0040's engine, on its own: a pure crate, `app-authz` (`crates/common/authz/`), with no poem or sqlx. It:

- loads a Module's policy files and checks them against Rust types;
- checks grants against them;
- compiles both into one rule set, swapped whole when grants change;
- evaluates it.

Tickets 0027–0029 put it to use; nothing calls it yet when this lands.

## Evidence (as of 2026-11-17)

- No crate in `crates/` does authorization. `AppError::Forbidden` exists and renders as `403` (`crates/common/web/src/error.rs`), and the MCP instructions already explain `403` (`crates/server/src/tools.rs`), but nothing returns it.
- `Claims` holds only `sub`, `iat` and `exp` (`crates/common/web/src/auth.rs`).
- The workspace has no YAML parser. `serde_yaml` is archived; pick a maintained one and pin it (ADR 0026).
- The workspace has no `arc-swap`; it, or a `RwLock<Arc<_>>`, is the swap.

## Proposed approach

1. **Types.**
   - `Principal` (`User { id, roles, attr }`, `System`).
   - `Value` (string, integer, boolean, null).
   - `Grant { role, resource, action, scope: Scope }`, where `Scope` is `Any` or `Named(String)`.
   - `Plan` (`Allowed`, `Denied`, `Where(Filter)`). `Filter` is the condition tree left once roles are known: `Eq`/`Ne`/`In` over `R.attr` and principal-resolved literals, `All`/`Any`/`None`, `True`/`False`.
2. **`Resource` trait.** `const KIND: &str`, `type Action` (a closed enum with a stable name per variant, e.g. via strum), `fn attr(&self, name: &str) -> Option<Value>`, and `const ATTRS: &[&str]` so loading can check names without a row.
3. **Parsing.** ADR 0040's three kinds under `apiVersion: auth.flagrante.io/v1`: `resourcePolicy`, `derivedRoles` and `defaultGrants`. Conditions are parsed by a small hand-written parser of the expression subset. Every refusal names the file and the line.
4. **Loading.** `Policies::builder("notes").resource::<Note>(include_str!(…)).derived_roles(include_str!(…)).default_grants(include_str!(…)).build()?`, one per Module, holding its gate (`module:<name>`) implicitly. It fails on:
   - an action not in `Note::Action`;
   - an attribute not in `ATTRS`;
   - an unknown or unimported derived role;
   - a scope (a derived role named by a grant) whose `parentRoles` isn't `["*"]`;
   - a policy or default grant for a resource the builder wasn't given (a Module's policies name only its own resources);
   - a condition or a scope on the gate.
5. **Checking a grant.** `policies.check(&grant) -> Result<(), GrantRefused>`: a known resource and action, and a scope that is `Any` or one of the resource's. This is what `access` calls before saving one (ticket 0029), and what `migrate` calls on each default.
6. **Compiling.** `RuleSet::compile(&[Policies], &[Grant])`. Each grant becomes a rule: `roles: [role]`, `actions: [action]`, `effect: ALLOW`, and its scope's condition, if any. A grant that no longer checks (its action or scope was removed by a later release) is skipped and returned in a list of stale grants, never failing the compile.
7. **Evaluating.** `rule_set.plan(&principal, resource, action) -> Plan`:
   - resolve the rules that apply from the principal's roles;
   - substitute `P.id` and `P.attr`;
   - fold to `Allowed`/`Denied` where no condition remains, `Where(filter)` otherwise.

   `filter.matches(&resource) -> bool` evaluates it against one row. Semantics are ADR 0040's: default deny, DENY beats ALLOW within a role, ALLOW across roles wins. `Principal::System` is always `Allowed`.
8. **Swapping.** `Authorizer` holds the current `Arc<RuleSet>` and `replace(rule_set)`. A plan uses one snapshot from start to end.
9. **Catalog.** `policies.catalog()`: resources, actions, scopes. Given a rule set, each action is marked unreachable when no rule could allow it. Ticket 0029 serves it.
10. **Tests.** Inline, table-driven:
    - each refusal in steps 4 and 5;
    - each combining rule;
    - wildcards, including `:`-segment ones;
    - scopes, as derived roles and as grants;
    - a plan that folds to `Allowed`/`Denied` against one that stays `Where`;
    - `matches` agreeing with the plan;
    - a stale grant skipped;
    - a swap not seen mid-plan.

## Acceptance criteria

- [ ] `app-authz` depends on no web or database crate, and `make check-module-deps` (or `deny.toml`) keeps it that way.
- [ ] The example files in ADR 0040, compiled with their default grants against a test `Note` resource, plan:
  - [ ] `user` + `edit` gives `Where(authorId == <id> AND NOT archived)`;
  - [ ] `user` + `create` gives `Allowed`;
  - [ ] no role gives `Denied`;
  - [ ] `System` gives `Allowed`.
- [ ] Adding the grant `support → note:view, any` and recompiling turns `support` + `view` from `Denied` to `Allowed`.
- [ ] Each refusal in steps 4 and 5, and each unsupported field, kind or expression, fails with the file and line (or the grant).
- [ ] A DENY rule overrides an ALLOW in the same role, including one from a grant, and not one in another role.
- [ ] `make check` passes.

## Open questions

- Which YAML crate. It must report line numbers for the refusals.

## Notes

- The glossary gains **Principal**, **Policy**, **Grant**, **Scope**, **Gate** and **Plan** in ticket 0027, once they're used.
