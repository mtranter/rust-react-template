---
id: 0073
name: "The assistant's approval card describes the business action in plain words, not JSON"
status: done
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: []
blocks: []
---

# The assistant's approval card describes the business action in plain words, not JSON

The people approving the assistant's writes aren't technical. Today the card for a pending write (`PendingWrite` in `frontend/src/features/assistant/conversation-view.tsx`) shows them three things:

- the raw tool name (`edit_note`);
- the operation's summary, which is written for an agent and mentions `If-Match`, `412` and so on;
- the arguments as pretty-printed JSON, including `expected_version`, `idempotency_key` and UUIDs.

Replace that with a description of what will happen, in business terms: the action, the record it acts on, and the business data it sets.

**The server builds the description from the operation and the arguments, never from the model's words.** Approval is the defence against prompt injection (Spec 0012, "Open risks"). If the model wrote the text, an injected instruction could make a delete read as "add milk". The card must describe exactly the call that approving runs.

## The description

The server builds a `proposal` for each call that is `awaiting_approval`. It's computed when the call is served, as `summary` is today (`tagged_with_summaries` in `web/handlers.rs`), so it needs no migration. It's sent on the `approvalRequired` event and on `ToolCallResponse` for pending calls.

A proposal has three parts.

- **`action`**: the operation id in sentence case, for example `edit_note` becomes "Edit note" and `assign_role` becomes "Assign role".
- **`target`** (optional): the record the call acts on, by its name, not its id, for example "Note: Shopping list".
  - Find it when the write's path has parameters and a read tool exists for the same path template, or for the longest prefix of it. For example, `PUT /notes/{id}` and `DELETE /notes/{id}` use `GET /notes/{id}`; a child path `/notes/{id}/items/{itemId}` uses the note.
  - Call that read as the user, through the same `AppTools` port and token.
  - Take the first non-empty string among the answer's `title`, `name`, `displayName`, `email` and `subject`.
  - The kind comes from the read's operation id: `get_note` becomes "Note".
  - When there's no such read, it's refused, or nothing in the answer names the record, there's no target. That isn't an error, and the card still shows.
- **`fields`**: the business data the call sends, in schema order. Each field has these parts:
  - `label`: the property's schema `title`, else its name in sentence case (`archiveAt` becomes "Archive at"). Nested objects are flattened as "Parent › Child".
  - `value`: the argument as given.
  - `format`: the schema's `format` when it's `date` or `date-time`, so the SPA can format the value for the user's locale.
  - `previous`: present when the target was read and has the same field with a different value, so an edit shows "Shopping list → Groceries".

  Fields leave out:
  - every argument sent as a header, which includes `expected_version` (`If-Match`) and `idempotency_key`;
  - path parameters, which are the target;
  - any property named `version`;
  - any value whose schema format is `uuid`.

`app_web::mcp::AgentTool` gains what the assistant needs to do this, such as each tool's path template and which of its arguments are sent as headers. It shouldn't parse the description text to get them.

## The card

- Its title is the action and the target, for example "Edit note: Shopping list". Under it: "The assistant wants to do this for you".
- The fields are a label/value list:
  - dates formatted with `Intl` in the browser's locale;
  - booleans as "Yes" or "No";
  - an empty value or `null` as "None";
  - lists as comma-separated text;
  - long text clamped to a few lines, with "Show more";
  - a changed field shown as its previous value struck through, then the new value.
- It shows no tool name, JSON, ids, versions or idempotency keys.
- If `proposal` is missing (a tool the app no longer has), fall back to "The assistant wants to make a change", with Approve disabled and Reject enabled.
- A tool call's row (reads, and decided writes) is headed by the same action text instead of the raw tool name. It shows the operation's `summary`, which `get_conversation` and the stream already provide, for reads that have no proposal. The JSON stays only inside the row's collapsed details.

## Acceptance criteria

- [ ] A pending call's `proposal` (`action`, optional `target`, `fields`) is built on the server from the operation's schema and the stored arguments. It's sent on `approvalRequired` and on `ToolCallResponse` for `awaiting_approval` calls. `make generate` has been run and `openapi.json` committed.
- [ ] Header arguments (`expected_version`, `idempotency_key`), path parameters, `version` and `uuid`-format values never appear in `fields`.
- [ ] Endpoint tests in `crates/platform/assistant/core/tests/assistant/behaviours/` (the driver's `AppOperations` gains whatever read and write it needs):
  - a pending edit has action "Edit …", the target's name, and fields without the version or the key;
  - a changed field carries `previous`;
  - a call whose target read is refused has no target and still shows its fields;
  - a pending delete names its target.
- [ ] The card renders as described, with no tool name, JSON or ids. A screen test (over the fake stream and a stored pending call) checks the action, target, a formatted date, a previous → new value, and the absence of `expected_version`, `idempotency_key` and the call's id. Rows are headed by the action text.
- [ ] Both are covered: a pending call when it streams, and when the conversation is reopened.
- [ ] `crates/AGENTS.md` "The assistant" says pending writes are described by a server-built proposal, not model text, and why. `make check-changed` passes.
