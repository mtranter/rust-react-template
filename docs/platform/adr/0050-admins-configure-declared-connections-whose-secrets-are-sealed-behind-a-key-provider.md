# Admins configure declared Connections, whose secrets are sealed behind a key provider

Apps built from the template call other systems' APIs, and the people who know those systems' URLs and keys are not the ones writing the code. In the software factory, an agent writes the module and its typed requests and responses. A non-technical admin of the running app supplies the base URL and credentials. Until now there was nowhere to put them except environment variables, which only a deployer can set.

**Code declares a Connection, admins fill it in** (the pattern of ADR 0040). A module declares each Connection it needs: a key (`crm`), a label, the auth style, any named non-secret fields, and an optional health-check request. Migrate records the declarations, the way it registers subscriptions (ADR 0014), so the admin screen can show every Connection the app expects, configured or not.

**A platform module, `connections`, stores them in Postgres.**
- A Connection's values are its base URL, its non-secret fields and its secrets. Secrets are sealed with AES-256-GCM before they are stored.
- The API never returns a secret's value. It only says whether the secret is set, when it was last set and by whom. Setting a secret again replaces it, which is also how a key is rotated.
- Each change publishes an event without the secret's value, so the outbox is the audit trail.
- Any admin may read and change Connections. There is no separate role (ADR 0041).

**Sealing goes through a port, `KeyProvider`.** It wraps and unwraps a per-secret data key (envelope encryption). The first adapter, `EnvKeyProvider`, holds one master key from `CONNECTIONS_MASTER_KEY` (32 bytes, base64). A KMS adapter is the next one, and it changes neither the stored format nor the module's API. With no key set, saving a secret answers `501 not_configured` (as in ADR 0048), and the rest of the app works.

**Auth styles:** none, bearer token, basic auth, an API key in a named header or query parameter, and OAuth 2 client credentials (with the token cached until it expires). User-delegated OAuth is left out.

**Modules call out through `connections`' module API** (ADR 0008). `client("crm")` returns an HTTP client with the base URL, auth, timeouts, tracing (ADR 0044) and redaction of secrets in logs and spans already applied. An unconfigured Connection is a domain error, `connection_not_configured`, not a 500. Calls to other systems happen in jobs (ADR 0043) or in the request that needs the answer, never inside an outbox handler's unit of work.

**The admin screen** lists each Connection with its status: not configured, untested, OK, or failing with the last error. Its form takes the URL, fields and secrets, and **Test connection** runs the health check and reports the result in plain language.

**The assistant and MCP never see a secret.** The operations that set secrets are kept off MCP (ADR 0034). Reads show only whether a secret is set, so an approval card (ADR 0073) can't leak a key.

**Dev and tests use stateful fakes.** For each integration the agent writes a fake of the other system: a small in-process server that keeps state in memory, like `fake-oidc`, serving the operations the module uses. Tests (ADR 0004) and `make dev` point the Connection's URL at it, and `make dev` seeds dev Connections to their fakes. Real sandbox credentials are entered by a person, like production ones.

## Considered options

- **Environment variables per integration.** Only a deployer can set them, and a non-technical admin can't, which is the whole point.
- **An external secret store (Secrets Manager or Vault) from day one.** It adds infrastructure to every app. `KeyProvider` leaves room for KMS, and a store adapter can come later behind the same module API.
- **Postgres `pgcrypto`.** The key would travel to the database in each query, and swapping in KMS would mean rewriting the SQL.
- **Free-form connections an admin creates.** Code couldn't rely on a Connection existing or on its shape. Declaring keeps the agent's code and the admin's values in step.

## Consequences

- Losing `CONNECTIONS_MASTER_KEY` loses every secret. Admins re-enter them. Rotating the master key re-wraps data keys and is a follow-up.
- Inbound webhooks, outbound event subscriptions to arbitrary URLs and user-delegated OAuth are not covered. Each would build on `connections`.
- Fakes are code the agent maintains, and they can drift from the real API. The health check against the real system is the guard.
