//! What AI agents get over MCP (ADR 0034): every operation of the API as a
//! tool, but those listed in `REST_ONLY`.

use crate::{API_BASE, API_TITLE, API_VERSION, Composition};
use app_web::Idempotent;
use app_web::mcp::{RestOnly, Surface};
use std::marker::PhantomData;

/// The tools of `C`'s operations.
pub struct Tools<C>(PhantomData<C>);

impl<C: Composition> Surface for Tools<C> {
    type Api = Idempotent<C::Endpoints>;
    const BASE: &'static str = API_BASE;
    const NAME: &'static str = API_TITLE;
    const VERSION: &'static str = API_VERSION;
    const INSTRUCTIONS: &'static str = INSTRUCTIONS;
    /// An operation an agent shouldn't be offered, by its operation id, with
    /// why (a browser-only redirect, a file upload). One whose body isn't JSON
    /// must be listed.
    const REST_ONLY: &'static [RestOnly] = &[];
}

/// Tool descriptions are the spec's summaries, written for REST, and a tool's
/// refusal carries the error body but no status: this translates. A
/// composed-app test fails when the spec declares a status not named here.
const INSTRUCTIONS: &str = "\
Each tool is an operation of this app's REST API, called as you, and answers as it does: \
a tool's result is the operation's response body, versions included.

- Reads answer the resource with its `version`. A tool changing something that exists takes \
`expected_version`: the `version` you last read of it. Send the one you read; never guess or \
increment it.
- A tool that changes something takes an optional `idempotency_key`, unique per intended \
change. Retrying with the same key and arguments replays the first answer instead of changing \
anything twice.
- A refusal is an error result whose text is the API's error body: branch on its `code`, not \
its `message`; `fields` names the arguments at fault.

Descriptions use the REST API's terms:
- `If-Match` means `expected_version`; `Idempotency-Key` means `idempotency_key`.
- The `ETag` header means the `version` field of the answer.
- A status means a refusal's `code`: `400` is `validation_error` or `malformed_request`, `401` \
is `unauthorized`, `403` is `forbidden`, `404` is `not_found`, `409` is `conflict` (or \
`idempotency_key_in_flight`: retry later), `412` is `precondition_failed` (a stale \
`expected_version`: nothing changed; read it again), `422` is `idempotency_key_reused` (the \
key was sent with other arguments), `428` is `precondition_required` (`expected_version` \
missing), `500` is `internal_error`, and `503` is `unavailable` (not ready to answer yet: \
retry later).
- A `204` answer is an empty result.";
