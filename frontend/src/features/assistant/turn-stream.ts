import { postAssistantConversationsByIdMessages } from "@/api/sdk.gen";
import type { TurnEventResponse } from "@/api/types.gen";
import { ifMatch } from "@/lib/if-match";

/**
 * Sends `text` to conversation `id` at `version` and yields the turn's events as they arrive.
 * Through the generated client, for its base URL, token and `ErrorBody`: a refusal before the
 * stream (`409`, `412`, `503`, ...) throws that body. The client types the `text/event-stream`
 * answer as an array of events but can't parse one, so it hands over the raw body (`parseAs:
 * "stream"`) for `readEvents` to read.
 */
export async function* sendMessage(
  id: string,
  version: string,
  text: string,
  signal: AbortSignal,
): AsyncGenerator<TurnEventResponse> {
  const { data } = await postAssistantConversationsByIdMessages({
    path: { id },
    body: { text },
    headers: ifMatch(version),
    parseAs: "stream",
    throwOnError: true,
    signal,
  });
  yield* readEvents(data as unknown as ReadableStream<Uint8Array>);
}

/**
 * The events of a server-sent event stream, each `data` parsed as JSON. The server sends only
 * `data` fields, one event per blank-line-separated block (poem's `EventStream`); any other field
 * is ignored, as the SSE format allows.
 */
export async function* readEvents(
  body: ReadableStream<Uint8Array>,
): AsyncGenerator<TurnEventResponse> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffered = "";
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      // A trailing `\r` waits for the next chunk, which may start with its `\n`.
      buffered = (buffered + decoder.decode(value, { stream: true })).replace(
        /\r\n|\r(?!$)/g,
        "\n",
      );
      let end = buffered.indexOf("\n\n");
      while (end >= 0) {
        const event = parse(buffered.slice(0, end));
        buffered = buffered.slice(end + 2);
        if (event) yield event;
        end = buffered.indexOf("\n\n");
      }
    }
    const last = parse(buffered);
    if (last) yield last;
  } finally {
    // Stops reading when the caller stops early (the view closed mid-turn): the turn runs on.
    reader.cancel().catch(() => {});
  }
}

function parse(block: string): TurnEventResponse | undefined {
  const data = block
    .split("\n")
    .filter((line) => line.startsWith("data:"))
    .map((line) => line.slice(line.startsWith("data: ") ? 6 : 5));
  return data.length ? (JSON.parse(data.join("\n")) as TurnEventResponse) : undefined;
}
