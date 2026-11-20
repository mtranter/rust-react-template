# The assistant calls the app's tools as its user

Spec 0012. The `assistant` platform module runs a model that acts in the app for the signed-in user. Something has to decide what the model can reach, as whom, and when it may change anything.

**Its tools are the app's tools.** A turn offers the model every tool `/mcp` lists (ADR 0034): the server's `Tools<C>` surface, minus `REST_ONLY`, so none of the assistant's own operations. Each has its operation's id, summary and input schema, generated from the spec, and the system prompt carries the MCP server's `INSTRUCTIONS`, which say how a tool answers. No tool is written for the assistant, so a module's new operation reaches it as it reaches `/mcp`. The server puts an `app_web::mcp::Toolbox` in the API's request data and serves it by that same API, so the module, which can't depend on `server`, gets the surface without knowing the composition.

**Every call is the user's.** A tool call is sent in-process through the endpoint `/api` is served by, as `/mcp` sends one, presenting the bearer token of the request running the turn (`Caller::token`). Auth, gates, policies, row scoping, `If-Match`, `Idempotency-Key` and the `ErrorBody` apply exactly as over REST: an operation the user may not perform is refused, and its `ErrorBody` (`403`, `404`, ...) goes back to the model as the tool's error result. A `401`, an expired token, ends the turn saying so. The assistant keeps no token past the request that brought it and never acts as `Principal::System`.

**Reads run; writes wait.** A call whose operation is a `GET` runs as soon as the model asks. Any other is stored `awaiting_approval` and not run, an approval-required event names it and its arguments, and the turn ends; several in one step are all pending. Deciding them is the user's (Spec 0012's decide operation).

**A turn is the server's, not the browser's.** It runs in a task of its own and stores each message as it is posted, so a closed tab doesn't stop it; the stream only follows it. On shutdown the server ends open streams (`app_web::Draining`, begun when the shutdown signal arrives) so graceful shutdown isn't held open by them.

**The model is behind the module's own seam.** `ChatModel` takes the module's types (messages, tools, tool calls and results) and streams text through a callback; rig (pre-1.0) and Bedrock stop at its adapter. The loop (model, reads, model, until words, a write or a limit) is the module's, not rig's agent loop, because rig's loop would run every tool it is given, and here a write must stop the turn and be stored for a decision.

## Considered options

- **Hand-picked tools written for the assistant.** A curated set could be described for a model and kept small, but every module would write each operation twice, the two would drift in rules and wording, and the assistant would lag every new operation. Generating from the spec gives it the same contract `/mcp` already gives agents outside the app.
- **A separate service calling the API over HTTP.** It would isolate the model's dependencies and failures, but it would hold or mint tokens to call back in, be one more thing to deploy (against ADR 0005's one artifact), and re-cross the network for every read. In-process calls through the same endpoint behave identically and cost nothing.
- **Act without approval.** Simplest for the user, but the model reads text other people may have written (a shared note), which could tell it to change or delete things. Approving every write before it runs is the defence; reads stay ungated, since they can only show the user what they may already see, and only to the model.
- **A service account or `Principal::System` for the assistant.** It would need its own grants and could reach what the user can't. Acting with the user's token means no second permission model and nothing to audit apart from the user's own actions, until Spec 0006 gives the agent an Actor of its own.

## Consequences

- The assistant can do exactly what its user can, and its calls are indistinguishable from the user's own, as over `/mcp`, until Spec 0006.
- Every operation is a tool, so the tool list grows with the app and costs input tokens on every model call. Spec 0008's hints, or an allow-list, may be needed; when its `readOnlyHint`/`destructiveHint` land they should replace the method as the approval rule.
- A write a turn wants is stored and the turn ends until the owner decides it (`decide_tool_call`); a new message rejects any still pending.
- Deciding takes no `If-Match`, unlike every other write (ADR 0016): the call's own status guards it, a decision on one no longer pending being `409`, so a client needn't hold the conversation's latest version to decide. And it is the one write that retries a lost race (a `412` from its save, as when two calls of one step are decided at once): it reloads and decides again, up to three times, which is safe because an approved call runs under an `idempotency_key` derived from its id (ADR 0032), so the retry replays the write rather than repeating it.
- Text inside tool results is data: the system prompt says so, and approval before writes is what actually guards against injected instructions.
- Tool calls go through the whole API stack (idempotency, error rendering, a request id and span of their own), so they show in traces and the `mcp.server.tool.calls` metric like `/mcp`'s.
- A turn outlives its request, so a server shut down mid-turn loses the rest of it; what was stored stays.
