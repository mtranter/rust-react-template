# MCP clients sign in through Cognito with Client ID Metadata Documents

[ADR 0034](0034-every-api-operation-is-an-mcp-tool-generated-from-the-spec.md) made every API operation an MCP tool, and the server already serves Protected Resource Metadata (RFC 9728) and a `WWW-Authenticate` challenge naming it. But an MCP client (Claude, VS Code, ChatGPT, MCP Inspector) also needs a `client_id` at the authorization server, and until November 2026 Cognito offered none of the MCP spec's ways to get one at runtime: each assistant needed an app client created by hand, or a registration shim of our own (API Gateway or Lambda calling `CreateUserPoolClient`).

**Cognito is the authorization server for MCP clients, as it is for the SPA (ADR 0042).** Cognito now advertises RFC 8414 metadata and supports both runtime ways in:

- **Client ID Metadata Documents (CIMD), preferred.** The client's `client_id` is an HTTPS URL serving its metadata; Cognito fetches and validates it at sign-in. No app client is created, so nothing piles up toward the pool's 10,000 app client quota, and nothing needs cleaning up. The MCP spec recommends it and deprecates DCR.
- **Dynamic Client Registration (RFC 7591), the fallback** for clients without CIMD, at Cognito's `/oauth2/register`. Registered clients count toward the quota and stay until deleted.

**The server is the resource server and rejects tokens not meant for it.** Cognito's access tokens carry `scope` and `client_id`, not `aud`, so the MCP endpoint is bound to a Cognito resource-server scope, `APP_MCP_SCOPE`: a token without it is refused `403` with an `insufficient_scope` challenge, and Protected Resource Metadata lists it in `scopes_supported`. A token whose `aud` names the MCP resource (RFC 8707, other issuers) is accepted too. Unset, the endpoint takes any token the server verifies, as before.

**`fake-oidc` emulates Cognito, not a generic provider.** It serves RFC 8414 metadata, `/oauth2/register` (open, or requiring a token with `aws.cognito/dcr/register`, Cognito's two modes), CIMD at `/authorize`, and Cognito-shaped access tokens (`scope`, `client_id`), so the journeys exercise the same flows production will.

## Considered options

- **Our own DCR shim (API Gateway and Lambda over `CreateUserPoolClient`).** Needed before November 2026, now redundant, and one more thing to secure.
- **Keycloak instead of `fake-oidc`.** Keycloak supports RFC 8414 and DCR, and CIMD only as an experimental feature (`--features=cimd`) needing client policies. It would make `make e2e` slower and heavier, still couldn't stand in for Cognito's user pool API (ADR 0048), and would test Keycloak's behaviour rather than Cognito's.
- **Pre-registered app clients only.** Many MCP clients can't take a `client_id`, and each new assistant would need an admin.

## Consequences

- The user pool needs the Essentials or Plus plan, managed login on every domain, a resource server with the MCP scope, and CIMD and DCR turned on (open DCR or a registration scope). This is infrastructure, done separately.
- Open DCR lets anyone add app clients to the pool. Prefer CIMD, and watch the quota or require a registration token.
- `fake-oidc` follows Cognito's documented behaviour, which may drift. A check against a real pool is a later step.
