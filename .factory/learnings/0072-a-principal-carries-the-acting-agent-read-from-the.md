# Learnings: 0072, a Principal carries the acting agent

- Process-wide auth settings read from config (`APP_BOOTSTRAP_ADMINS`, now `APP_FIRST_PARTY_CLIENTS`) live on the `Authorizer` (`with_*` builders), set in `crates/server/src/main.rs`. The default `PrincipalResolver` is `impl PrincipalResolver for Authorizer`, and `access`' `RecordingSignIns` delegates to it, so anything the resolver needs goes on the `Authorizer` and both resolvers pick it up.
- Config values that can look numeric or boolean (client ids, subjects) must be listed in `RAW` in `crates/server/src/config.rs`, or figment parses them.
- The actor logic is `Claims::actor(is_first_party)` in `crates/common/web/src/auth.rs`; `app_authz` stays pure and only holds `Actor` and the first-party list.
- In `auth.rs` tests a `#[handler] fn roles` is in scope, so a destructured binding named `roles` resolves to the handler: name it something else.
- `access`' test driver has `sign_in_as_agent(client)` for a client-credentials token (sub = client_id).
