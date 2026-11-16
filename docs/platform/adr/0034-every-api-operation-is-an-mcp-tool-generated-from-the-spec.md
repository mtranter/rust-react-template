# Every API operation is an MCP tool, generated from the spec

AI agents should be able to do whatever the REST API offers, over MCP's streamable HTTP transport, without a custom integration, and a module author should get this for free. So the server answers MCP at `/mcp`, and every operation of the Spec is a Tool unless deliberately kept off:

- **The tools are listed from the API type.** `app_web::mcp::OpenApiTools<S>` reads the operations of the `Idempotent<Endpoints>` the server serves, through poem-openapi's static `meta()` and `register(..)`. A tool's name is the operation's `operation_id`, its description the operation's summary and description, its input schema an object of the operation's arguments (path and query parameters by name, `If-Match` as `expected_version`, other headers in snake case, such as `idempotency_key`, and the request body as `body`), and its output schema the 2xx body's, with the types it names under `$defs`.
- **A call is the operation's request, sent in-process through the endpoint `/api` is served by**, with its middleware and each module's data, presenting the session's bearer token. So a tool and its operation run the same code: auth, versions, `Idempotency-Key`, validation and the `ErrorBody`. A 2xx answers its body as structured content and text; anything else answers its `ErrorBody` as an error result. The bridge reimplements none of it.
- **Every `/mcp` request needs a valid token**, though most of REST is anonymous: an agent always acts for someone. A session is bound to the token subject that opened it, and each request's token becomes the one its calls present, so a refreshed token keeps a long session working and an expired one ends it. Without a token, a client is told where to sign in: Protected Resource Metadata (RFC 9728) naming `APP_JWT_ISSUER`, when one is set.
- **Tool descriptions keep the REST wording** of the Spec, which the Generated Client shares, and the server's MCP instructions translate it (`If-Match` is `expected_version`, `412` is `precondition_failed`, and so on).
- **An operation is kept off MCP** only by listing its id in `REST_ONLY` (`crates/server/src/tools.rs`) with a reason. One that can't be a tool, such as one taking a body that isn't JSON, must be listed: a test fails otherwise.

Operation ids are a public contract, as tool names. The Generated Client keeps deriving its names from method and path (`operationId: false`), so the two stay independent.

## Considered options

- **Hand-written tools per module** (as Flagrante, the project this was taken from, does). Tools can be shaped for agents, but every operation needs boilerplate kept in step with its REST twin, the surface ends up partial in practice (13 tools for a larger API there), and calling services directly bypasses the HTTP middleware, so idempotency had to be implemented a second time.
- **Generated tools with hand-written overrides.** More to build than any tool needs yet: a better summary is the first fix for a tool that reads badly.
- **A hosted MCP generated from the OpenAPI document** (for example, Kinde's). It runs outside the binary, against ADR 0005's one artifact, and in front of the API rather than through it.

## Consequences

- A new operation is a tool with no code, and so is a scaffolded module's. Its id and summary need writing with agents in mind; the composed-app test fails on an operation lacking either, and lists every tool so the surface shows in a diff.
- MCP gets whatever REST gets: when authorization is built, tools enforce it because they call the same operations with the session's token. Until then any signed-in agent can, for example, register users, as any REST client can.
- An in-process call skips the outer middleware (CORS, the access log); it carries the MCP request's request id.
- Sessions live in the process that opened them, and are forgotten after 5 minutes idle: behind several instances, the load balancer routes by `Mcp-Session-Id`.
- A new refusal status needs a line in the instructions, or a composed test fails, so no status reaches agents untranslated.
- Not built: anonymous sessions, MCP resources, prompts and streaming results, the app issuing tokens or registering clients, and sessions shared across instances.
