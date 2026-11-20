import type {
  ConversationResponse,
  ConversationSummaryResponse,
  DecideToolCallRequest,
  ErrorBody,
  ListResultConversationSummaryResponse,
  SendMessageRequest,
  TurnEventResponse,
} from "@/api";
import { http, HttpResponse } from "msw";

export const assistantOn = http.get("*/api/assistant/status", () =>
  HttpResponse.json({ on: true }),
);

/** Asked which roles there are, the assistant looked them up and answered. */
export const rolesConversation: ConversationResponse = {
  id: "conversation-roles",
  title: "Which roles are there?",
  createdAt: "2026-01-01T09:00:00Z",
  messages: [
    {
      kind: "user",
      id: "message-1",
      postedAt: "2026-01-01T09:00:00Z",
      text: "Which roles are there?",
    },
    {
      kind: "toolCalls",
      id: "message-2",
      postedAt: "2026-01-01T09:00:01Z",
      toolCalls: [
        {
          id: "call-1",
          tool: "list_roles",
          arguments: { limit: 20 },
          status: "ran",
          result: { items: [{ name: "admin" }, { name: "user" }] },
        },
        {
          id: "call-2",
          tool: "delete_role",
          arguments: { name: "admin" },
          status: "rejected",
          error: { code: "conflict", message: "the user declined this action" },
        },
      ],
    },
    {
      kind: "assistant",
      id: "message-3",
      postedAt: "2026-01-01T09:00:02Z",
      text: "There are two roles: admin and user.",
    },
  ],
  version: "3",
};

/** Started after the other, and never written in. */
export const untitledConversation: ConversationResponse = {
  id: "conversation-untitled",
  title: "",
  createdAt: "2026-01-02T09:00:00Z",
  messages: [],
  version: "1",
};

function summary({ messages: _, ...conversation }: ConversationResponse) {
  return conversation satisfies ConversationSummaryResponse;
}

function notFound() {
  return HttpResponse.json(
    { code: "not_found", message: "conversation not found", requestId: "test" } satisfies ErrorBody,
    { status: 404 },
  );
}

/** A turn's stream that the test plays out: `emit` sends events as they come, `end` closes it. */
export type ScriptedTurn = ReturnType<typeof scriptedTurn>;

export function scriptedTurn() {
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  const body = new ReadableStream<Uint8Array>({
    start: (controller) => {
      stream = controller;
    },
  });
  const encoder = new TextEncoder();
  return {
    body,
    emit: (...events: TurnEventResponse[]) => {
      for (const event of events) {
        stream.enqueue(encoder.encode(`data: ${JSON.stringify(event)}\n\n`));
      }
    },
    end: () => stream.close(),
  };
}

/**
 * The assistant as the server answers it for `conversations` (changed in place): the caller's list
 * newest first, each one with its `ETag`, deletes guarded by `If-Match`, and a started one given the
 * next id. `ifMatches` records each delete's `If-Match`. A message sent at the conversation's
 * version is stored, its version moved on, and answered with the next of `turns`; `sent` records
 * each one's text and `If-Match`. A decision on a call awaiting one is recorded on the call, the
 * version moved on, and answered with the next of `turns` too (`decided` records each); on any
 * other call it is a `409`.
 */
export function assistantApi(conversations: ConversationResponse[], turns: ScriptedTurn[] = []) {
  const ifMatches: (string | null)[] = [];
  const sent: { text: string; ifMatch: string | null }[] = [];
  const decided: { callId: string; decision: DecideToolCallRequest["decision"] }[] = [];
  let started = 0;
  const handlers = [
    assistantOn,
    http.get("*/api/assistant/conversations", () => {
      const items = [...conversations]
        .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
        .map(summary);
      return HttpResponse.json({
        items,
        total: items.length,
        limit: 100,
        offset: 0,
      } satisfies ListResultConversationSummaryResponse);
    }),
    http.post("*/api/assistant/conversations", () => {
      started += 1;
      const conversation: ConversationResponse = {
        id: `conversation-started-${started}`,
        title: "",
        createdAt: new Date(Date.UTC(2026, 1, started)).toISOString(),
        messages: [],
        version: "1",
      };
      conversations.push(conversation);
      return HttpResponse.json(conversation, { headers: { ETag: '"1"' } });
    }),
    http.get("*/api/assistant/conversations/:id", ({ params }) => {
      const conversation = conversations.find((c) => c.id === params.id);
      if (!conversation) return notFound();
      return HttpResponse.json(conversation, {
        headers: { ETag: `"${conversation.version}"` },
      });
    }),
    http.delete("*/api/assistant/conversations/:id", ({ params, request }) => {
      const ifMatch = request.headers.get("If-Match");
      ifMatches.push(ifMatch);
      const at = conversations.findIndex((c) => c.id === params.id);
      if (at < 0) return notFound();
      if (ifMatch !== `"${conversations[at].version}"`) {
        return HttpResponse.json(
          {
            code: "precondition_failed",
            message: "the conversation was changed meanwhile",
            requestId: "test",
          } satisfies ErrorBody,
          { status: 412 },
        );
      }
      conversations.splice(at, 1);
      return new HttpResponse(null, { status: 204 });
    }),
    http.post("*/api/assistant/conversations/:id/messages", async ({ params, request }) => {
      const { text } = (await request.json()) as SendMessageRequest;
      const ifMatch = request.headers.get("If-Match");
      sent.push({ text, ifMatch });
      const conversation = conversations.find((c) => c.id === params.id);
      if (!conversation) return notFound();
      if (ifMatch !== `"${conversation.version}"`) {
        return HttpResponse.json(
          {
            code: "precondition_failed",
            message: "the conversation was changed meanwhile",
            requestId: "test",
          } satisfies ErrorBody,
          { status: 412 },
        );
      }
      const turn = turns.shift();
      if (!turn) throw new Error("no turn scripted for this message");
      conversation.messages.push({
        kind: "user",
        id: `message-${conversation.messages.length + 1}`,
        postedAt: "2026-01-03T09:00:00Z",
        text,
      });
      conversation.title ||= text;
      conversation.version = String(Number(conversation.version) + 1);
      return new HttpResponse(turn.body, { headers: { "Content-Type": "text/event-stream" } });
    }),
    http.post(
      "*/api/assistant/conversations/:id/tool-calls/:callId/decision",
      async ({ params, request }) => {
        const { decision } = (await request.json()) as DecideToolCallRequest;
        const callId = String(params.callId);
        decided.push({ callId, decision });
        const conversation = conversations.find((c) => c.id === params.id);
        if (!conversation) return notFound();
        const call = conversation.messages
          .flatMap((message) => (message.kind === "toolCalls" ? message.toolCalls : []))
          .find((c) => c.id === callId);
        if (!call) return notFound();
        if (call.status !== "awaiting_approval") {
          return HttpResponse.json(
            {
              code: "conflict",
              message: "the tool call isn't awaiting a decision",
              requestId: "test",
            } satisfies ErrorBody,
            { status: 409 },
          );
        }
        const turn = turns.shift();
        if (!turn) throw new Error("no turn scripted for this decision");
        call.status = decision === "approve" ? "approved" : "rejected";
        conversation.version = String(Number(conversation.version) + 1);
        return new HttpResponse(turn.body, { headers: { "Content-Type": "text/event-stream" } });
      },
    ),
  ];
  return { handlers, ifMatches, sent, decided };
}
