---
id: 0010
name: Every API operation is also an MCP tool, generated from the spec
status: done
priority: P1
size: L
blocked_by: []
blocks: []
source: conversation 2026-11-15 (bring flagrent's MCP server into the template, with every operation a tool by default)
---

# Every API operation is also an MCP tool, generated from the spec

## Description

AI agents should be able to do everything the REST API offers, over MCP's streamable HTTP transport, without a custom integration. A module author should get this for free: adding a REST operation adds its tool, with no tool code to write and nothing to keep in step.

Flagrent already serves MCP (`../flagrent/crates/common/web/src/mcp/`). There, each module hand-writes a `Tools` impl in `web/tools.rs` that calls its service the way its REST operation does. The template takes flagrent's protocol plumbing, its session binding and its sign-in discovery. It drops the hand-written tools: the tools are generated from the poem-openapi operations, and each call is sent in-process through the same REST endpoint. As a result, MCP and REST behave the same because they run the same code: auth, `If-Match`, `Idempotency-Key`, validation and error bodies.

## User stories

1. As an AI agent, I want to connect to the app over MCP's streamable HTTP transport, so that I can use it without a custom integration.
2. As an AI agent, I want a tool for every API operation, described and typed as the API is, so that I can do anything a client of the API can.
3. As an AI agent, I want my changes to name the version I read and to accept an idempotency key, as the API's do, so that my writes are as safe as a human's.
4. As an AI agent, I want each refusal as the API's error body, so that I can tell a stale version from a validation error.
5. As a user, I want my agent to act as me, with my token, so that `me` operations are mine and nobody else can use my session.
6. As a user of an MCP client like Claude Code, I want the server to tell my client where to sign in, so that I don't paste tokens by hand.
7. As a module author, I want a new REST operation to become a tool with no extra code, so that the MCP surface never lags the API.
8. As a module author, I want to keep an operation off MCP deliberately, with a reason, so that a browser-only operation isn't offered to agents.

## Evidence (as of 2026-11-15, `main` at `48ff86a`)

- **Flagrent's server**, `../flagrent/crates/common/web/src/mcp/`:
  - `mod.rs`: `endpoint(..)` over `poem_mcpserver::streamable_http`, served at `/mcp`. Its `MemberSessions` middleware authenticates every request, not only the first, and binds a session to the Tenant and Member that opened it. Another caller's session id answers `not_found`.
  - `answer.rs`: `ToolAnswer<T>` answers the REST body as structured content, or an `ErrorBody` as an error result.
  - `arguments.rs`: `Body<T>`, `ExpectedVersion` (MCP's `If-Match`) and `IdempotencyKey`.
  - `idempotency.rs`: `IdempotentTools`, which re-implements `Idempotency-Key` per tool call. This is needed only because its tools bypass the HTTP middleware.
  - `schema.rs`: turns a poem-openapi type's schema into a JSON Schema with `$defs`.
  - `discovery.rs`: Protected Resource Metadata (RFC 9728) and the `WWW-Authenticate` challenge.
  - `../flagrent/crates/server/src/modules.rs` merges the modules' tools into `ServerTools` by hand, and `scripts/check-modules.sh` checks that it does.
- **Tools cover only part of REST there.** `crates/server/tests/composed_app.rs::one_mcp_endpoint_offers_every_modules_tools` lists 13 tools for a larger API.
- **This is feasible here as a generic bridge.**
  - `poem_mcpserver::tool::Tools::list()` and `instructions()` are static, as are `poem_openapi::OpenApi::meta()` and `register(..)`.
  - So `OpenApiTools<A: OpenApi>` can list tools from the API type alone. `MetaOperation` carries `operation_id`, `summary`, `description`, `params` (name, `in_type`, schema, required), `request` and `responses`.
  - `Tool { name: &'static str, description: &'static str, .. }` matches `MetaOperation`'s `&'static str` fields.
  - `app_web::Idempotent<T>` already adds `Idempotency-Key` to every unsafe operation's meta, so the bridge sees it.
- **The template's API** (`frontend/openapi.json`) has 14 operations.
  - Every operation has a doc-comment summary.
  - None has an `operationId`. The generated client derives names such as `postNotes` and `patchUsersMe`, which 4 feature files use.
  - `@hey-api/openapi-ts` 0.64's SDK plugin has `operationId?: boolean` ("Use operation ID to generate operation names?", default `true`).
- **Auth here is optional.**
  - Only `/users/me` requires a bearer token (`JwtAuth`, ADR 0025).
  - `Idempotency-Key` is scoped to the token's subject, or to one anonymous scope (`crates/common/web/src/idempotency.rs`).
  - There is no `PublicUrl` or `APP_PUBLIC_URL` yet (flagrent: `crates/common/web/src/public_url.rs`).

## Proposed approach

### The bridge: `app_web::mcp`

- **One endpoint at `/mcp`**, which `build_app` mounts beside `/api`, `/docs` and `/health`. `poem-mcpserver` (`0.3`, `streamable-http`) speaks the protocol, as in flagrent. Add it to the workspace dependencies.
- **`OpenApiTools<A: OpenApi>` lists one tool per operation of `A`**, the same `Idempotent<Endpoints>` type `build_service()` serves. Operations on a static exclusion list are left out (see "Opting out").
  - **Name:** the operation's `operation_id`.
  - **Description:** its summary and description, as in the spec.
  - **Input schema:** an object with one property per argument (see "Arguments").
  - **Output schema:** its 2xx response's JSON schema, made an object at the root as MCP requires. Copy `schema.rs` from flagrent to translate `#/components/schemas/` to `$defs`. An operation answering no body (`204`) declares no output schema.
- **Arguments:**
  - **Path and query parameters** keep their names (`id`, `item_id`, `q`, `limit`, `offset`, `sort`) and schemas. An array such as `sort` repeats in the query string.
  - **`If-Match` becomes `expected_version`**: the `version` the agent last read. The description says a stale one is refused and changes nothing.
  - **`Idempotency-Key` becomes `idempotency_key`**, with the description `Idempotent` already writes.
  - **The request body becomes `body`**, required as the operation requires it.
  - **Any other header** becomes its name in snake case.
  - **Two arguments with one name** fail the tool listing test, not a call.
- **A call is a request to the REST endpoint, in-process.**
  - The bridge builds the request from the arguments:
    - The method and the path, with path parameters substituted and percent-encoded, under `API_BASE`.
    - The query string.
    - The `If-Match` and `Idempotency-Key` headers, and `Authorization: Bearer <the session's token>`.
    - The JSON body.
  - It calls the same endpoint `/api` is served by, with its middleware: `Idempotency`, `catch_all_error(render_error)`, each module's `provide`, and the verifier. It doesn't call the whole app, so a call never re-enters `/mcp`.
  - So the bridge reimplements no idempotency (flagrent's `IdempotentTools` isn't ported), versions or validation. A wrongly typed argument is refused the way the operation refuses it.
- **What a tool answers:**
  - **A 2xx with a body:** the body as structured content and as text.
  - **A 2xx without one:** an empty text result.
  - **Anything else:** the `ErrorBody` as text, with `isError: true`. This is flagrent's `answer.rs`, without the generic `ToolAnswer<T>`.
- **Instructions:** one static text, written in `crates/server`.
  - Tools answer as the REST API does, versions included.
  - A tool changing something that exists takes `expected_version`.
  - A tool that changes something takes an optional `idempotency_key`.
  - A refusal is the API's error body, with its `code`.
- **The instructions translate the HTTP terms in tool descriptions.** Summaries stay written for REST, since they are the spec's and the client's too. Tool descriptions are those summaries, so they mention headers and status codes. A tool refusal carries only the error body, with no status. So the instructions say:
  - `If-Match` means `expected_version`. `Idempotency-Key` means `idempotency_key`.
  - The `ETag` header means the `version` field of the answer.
  - Each status means a refusal's `code`: `400` is `validation_error` or `malformed_request`, `401` is `unauthorized`, `403` is `forbidden`, `404` is `not_found`, `409` is `conflict`, `412` is `precondition_failed` (stale `expected_version`; nothing changed), `422` is `idempotency_key_reused`, and `428` is `precondition_required` (`expected_version` missing).
  - A `204` answer is an empty result.

  A composed test fails when the spec declares a refusal status the instructions don't map. That way a new status can't reach agents untranslated.
- **Static constraint:** `Tools::list()` and `instructions()` take no `self`, so the API type, the exclusion list and the instructions are type-level. For example, a trait `crates/server` implements for its `Endpoints`, with associated consts. The implementer chooses the shape.

### Operation ids

- **Every operation declares `#[oai(operation_id = "…")]`.** The id is snake_case, verb first, and unique: it is the tool's name and a public contract. The proposed ids:

  | Operation | Id |
  |---|---|
  | `POST /users` | `register_user` |
  | `GET /users` | `list_users` |
  | `GET /users/me` | `get_me` |
  | `PATCH /users/me` | `rename_me` |
  | `DELETE /users/me` | `delete_me` |
  | `GET /users/{id}` | `get_user` |
  | `POST /notes` | `create_note` |
  | `GET /notes` | `list_notes` |
  | `GET /notes/{id}` | `get_note` |
  | `PATCH /notes/{id}` | `edit_note` |
  | `DELETE /notes/{id}` | `delete_note` |
  | `POST /notes/{id}/items` | `add_note_item` |
  | `DELETE /notes/{id}/items/{item_id}` | `remove_note_item` |
  | `GET /note-search` | `search_notes` |

- **A test on the composed spec fails** when an operation lacks an id or a summary, or when two operations share an id.
- **The generated client keeps its names.** Add `{ name: "@hey-api/sdk", operationId: false }` to `frontend/openapi-ts.config.ts`'s plugins, so names keep deriving from method and path, and tool names and client names stay independent. Checked on 2026-11-15 with 0.64.0: with the 14 ids in the spec and this setting, the generated `sdk.gen.ts`, `types.gen.ts` and `@tanstack/react-query.gen.ts` are byte-for-byte today's. The TanStack plugin takes its names from the SDK plugin. Without the setting, every name changes (`getUsersOptions` → `listUsersOptions`, `PostUsersData` → `RegisterUserData`).
- `module-template`'s handlers and `scripts/new-module.sh` declare ids too, so a scaffolded module's operations are tools at once.

### Sessions and auth

- **Every `/mcp` request needs a valid bearer token.** This is stricter than REST, which is mostly anonymous: an agent always acts for someone.
  - A request without a token, or with an invalid one, gets the `401` `ErrorBody`.
  - When `APP_JWT_ISSUER` is set, the `401` also carries `WWW-Authenticate: Bearer resource_metadata="<public URL>/.well-known/oauth-protected-resource/mcp"`.
- **A session is bound to the token subject that opened it.** Port flagrent's `MemberSessions` as `SubjectSessions`:
  - It authenticates every request, not only `initialize`.
  - Another subject's session id answers `not_found`, as a session never opened does.
  - A session idle for 5 minutes is forgotten. `DELETE` closes it.
  - Each request's token becomes the session's current token, and the session's calls forward that token. So a refreshed token keeps a long session working, and an expired one ends it.
- **Sessions live in the process that opened them.** Behind several instances, the load balancer routes by `Mcp-Session-Id`, as in flagrent. Record this in `crates/AGENTS.md`.
- **Discovery:** port flagrent's `discovery.rs` and `public_url.rs`, with a new optional `APP_PUBLIC_URL`.
  - `GET /.well-known/oauth-protected-resource` and `GET /.well-known/oauth-protected-resource/mcp` answer `{ resource, authorization_servers: [APP_JWT_ISSUER], bearer_methods_supported: ["header"] }`.
  - The public URL is `APP_PUBLIC_URL`, or is derived from `Host` and `X-Forwarded-*` when that is unset.
  - Without an issuer (HS256 in development), both paths answer `404`.

### Opting out

- **An operation is a tool unless it is listed in one static exclusion list in `crates/server`**, by its operation id, with a comment giving the reason (for example, a browser-only redirect or a file upload).
- The list starts empty: all 14 operations are tools.
- An operation whose request body isn't JSON, or whose response is a stream, must be excluded. The tool listing test fails otherwise.
- **`register_user` is a tool**, so any signed-in agent can register users, as any REST client can today. Authorization isn't built yet. When it is, `/mcp` gets it for free, because tool calls go through the REST endpoint with the session's token. Revisit this then rather than excluding the operation now.

### Recording it

- **ADR 0034, "Every API operation is an MCP tool, generated from the spec."** It weighs three options:
  - hand-written tools per module (flagrent's way: tools shaped for agents, but per-operation boilerplate, a partial surface in practice, and a second idempotency implementation);
  - generated tools with hand-written overrides (more to build than anything needs yet);
  - a hosted MCP generated from the OpenAPI document (for example, Kinde Secure MCP: outside the binary, and outside ADR 0005's one artifact).
- **`crates/AGENTS.md`:**
  - Every operation is a tool by default. Give each an `operation_id` and a summary written for an agent as much as for a person. Opt out only through the exclusion list, with a reason.
  - Sessions are per process.
  - The table row "MCP tools for an operation" points to "nothing to write: see `app_web::mcp`".
- **`GLOSSARY.md`:** **Tool**, an API operation as an MCP client calls it.
- **`AGENTS.md`:** a line under "Architecture and conventions": spec → client → tools.

## Acceptance criteria

- [ ] `/mcp` serves MCP's streamable HTTP transport. `tools/list` names exactly the 14 operations above, each with its summary as its description and with input and output schemas from the spec.
- [ ] Each tool call goes through the REST endpoint in-process. A tool answers the body the same REST request answers, and its refusal is the same `ErrorBody` with `isError: true`.
- [ ] `expected_version` and `idempotency_key` work as `If-Match` and `Idempotency-Key`. A stale version is refused as `precondition_failed` and changes nothing. A retried keyed call replays the first answer and runs once. The same key with other arguments is refused as `idempotency_key_reused`.
- [ ] Every `/mcp` request needs a valid token, and a session can't be used with another subject's token. `get_me` answers the token's user.
- [ ] With `APP_JWT_ISSUER` set, the Protected Resource Metadata is served on both paths and a token-less request is challenged. Without it, both paths answer `404`.
- [ ] Every operation declares a unique `operation_id`, and a test fails on one without an id or a summary. `make generate` leaves `frontend/src/api/` unchanged, and `make check-spec` passes.
- [ ] A module scaffolded with `make new-module` exposes its operations as tools with no further code.
- [ ] ADR 0034, `crates/AGENTS.md`, `GLOSSARY.md` and `.env.example` (`APP_PUBLIC_URL`) are updated. `make check` passes.

## Testing

- **`app-web` unit tests (the bridge)**, against a small `OpenApi` of their own:
  - Arguments map to the method, path, query, headers and body.
  - A `204` answers empty, and a refusal answers its `ErrorBody`.
  - Two arguments with one name are caught.
  - Port flagrent's session tests from `mod.rs`: a session acts as its opener, another subject's token finds no session, and a token-less request is refused and challenged.
- **`TestApi`** gains `mcp_session(token)` and `McpSession` (`tools()`, `call(name, arguments)`, `presenting(token)`, `close()`), ported from flagrent's `crates/common/web/src/testing.rs`.
- **Composed** (`crates/server/tests/composed_app.rs`):
  - One test lists the tools by name, so the MCP surface is visible in a diff.
  - One test checks every spec operation has a unique id and a summary.
  - One test checks the instructions map every refusal status the spec declares.
- **Journeys** (`crates/server/tests/journeys/`): the driver gains `as_agent(user)`, a `Session` that speaks MCP. One journey per concern, through the composed app:
  - An agent writes a note and edits it.
  - An agent's stale edit is refused and changes nothing.
  - A retried keyed create creates one note.
  - An agent's `get_me` is its user.
  - A token-less session is refused.
- **Module tests are unchanged.** A module has no tool code of its own, so the bridge's tests and the journeys cover it.

## Out of scope

- Hand-written tools, or overriding a generated tool. Add these when a tool needs shaping that a better summary can't give.
- MCP resources, prompts, sampling, and streaming tool results.
- The app issuing tokens or acting as an OAuth authorization server, Dynamic Client Registration, and scopes (flagrent's `mcp-auth` ticket 03).
- Sessions shared across instances.
- Anonymous MCP sessions, even for operations REST serves anonymously.

## Resolution

Built as proposed, and recorded as ADR 0034. `app_web::mcp` generates one tool per operation of the API type from poem-openapi's meta (`OpenApiTools<S: Surface>`), sends each call in-process through the endpoint `/api` is served by, binds sessions to the token subject that opened them (`SubjectSessions`), and answers Protected Resource Metadata when `APP_JWT_ISSUER` is set (`APP_PUBLIC_URL` overrides the derived origin). The server's `Surface` is `crates/server/src/tools.rs`, with an empty `REST_ONLY`. Idempotency moved from the app to the API endpoint so that tool calls go through it too. The 14 operation ids are listed in the composed-app test, and `make new-module` adds a new module's ids there.

Left as it was: `get_me`'s description still carries a note for developers about route order. Rewording it changes the generated client.
