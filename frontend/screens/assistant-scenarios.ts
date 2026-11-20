import type {
  ConversationResponse,
  ConversationSummaryResponse,
  ListResultConversationSummaryResponse,
  TurnEventResponse,
} from "@/api";
import { http, HttpResponse } from "msw";
import type { Scenario } from "./scenarios.ts";

// The platform's assistant screens, which `make init` keeps. Like scenarios.ts, run by Node's type
// stripping: so only package imports, and types from `@/`.

const assistantOn = http.get("*/api/assistant/status", () => HttpResponse.json({ on: true }));

const roles: ConversationResponse = {
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
          arguments: {},
          status: "ran",
          result: { items: [{ name: "admin" }, { name: "user" }], total: 2 },
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
  version: "1",
};

const untitled: ConversationResponse = {
  id: "conversation-untitled",
  title: "",
  createdAt: "2026-01-02T09:00:00Z",
  messages: [],
  version: "1",
};

function conversationsOf(conversations: ConversationResponse[]) {
  const items = conversations.map(
    ({ messages: _, ...summary }): ConversationSummaryResponse => summary,
  );
  return [
    assistantOn,
    http.get("*/api/assistant/conversations", () =>
      HttpResponse.json({
        items,
        total: items.length,
        limit: 100,
        offset: 0,
      } satisfies ListResultConversationSummaryResponse),
    ),
    ...conversations.map((conversation) =>
      http.get(`*/api/assistant/conversations/${conversation.id}`, () =>
        HttpResponse.json(conversation),
      ),
    ),
  ];
}

/** A turn caught mid-stream: `events` sent, and the stream left open. */
function turnStreaming(events: TurnEventResponse[]) {
  return http.post("*/api/assistant/conversations/:id/messages", () => {
    const encoder = new TextEncoder();
    const body = new ReadableStream<Uint8Array>({
      start: (controller) => {
        for (const event of events) {
          controller.enqueue(encoder.encode(`data: ${JSON.stringify(event)}\n\n`));
        }
      },
    });
    return new HttpResponse(body, { headers: { "Content-Type": "text/event-stream" } });
  });
}

const failed: ConversationResponse = {
  id: "conversation-failed",
  title: "Summarise my week",
  createdAt: "2026-01-03T09:00:00Z",
  messages: [
    {
      kind: "user",
      id: "message-1",
      postedAt: "2026-01-03T09:00:00Z",
      text: "Summarise my week",
    },
    {
      kind: "error",
      id: "message-2",
      postedAt: "2026-01-03T09:00:01Z",
      text: "The assistant couldn't answer: the model is throttled.",
    },
  ],
  version: "2",
};

const awaitingApproval: ConversationResponse = {
  id: "conversation-awaiting",
  title: "Delete the admin role",
  createdAt: "2026-01-04T09:00:00Z",
  messages: [
    {
      kind: "user",
      id: "message-1",
      postedAt: "2026-01-04T09:00:00Z",
      text: "Delete the admin role",
    },
    {
      kind: "toolCalls",
      id: "message-2",
      postedAt: "2026-01-04T09:00:01Z",
      toolCalls: [
        {
          id: "call-1",
          tool: "delete_role",
          summary: "Delete a role and every grant and assignment of it.",
          arguments: { name: "admin", expected_version: "3" },
          status: "awaiting_approval",
        },
      ],
    },
  ],
  version: "3",
};

export const assistantScenarios: Scenario[] = [
  {
    name: "assistant",
    route: "/assistant",
    handlers: conversationsOf([untitled, roles]),
    shows: (page) => page.getByRole("navigation", { name: "Conversations" }),
  },
  {
    name: "assistant-empty",
    route: "/assistant",
    handlers: conversationsOf([]),
    shows: (page) => page.getByText("No conversations yet."),
  },
  {
    name: "assistant-conversation",
    route: "/assistant?conversation=conversation-roles",
    handlers: conversationsOf([untitled, roles]),
    steps: async (page) => {
      await page.getByRole("button", { name: /list_roles/ }).click();
    },
    shows: (page) => page.getByText("Result"),
  },
  {
    name: "assistant-streaming",
    route: "/assistant?conversation=conversation-untitled",
    handlers: [
      turnStreaming([
        { kind: "toolCallStarted", id: "call-1", tool: "list_roles", arguments: {} },
        { kind: "toolResult", id: "call-1", result: { items: [{ name: "admin" }], total: 1 } },
        { kind: "text", text: "There is **one** role so far:\n\n- `admin`\n\nI'm checking" },
      ]),
      ...conversationsOf([untitled, roles]),
    ],
    steps: async (page) => {
      await page.getByRole("textbox", { name: "Message" }).fill("Which roles are there?");
      await page.getByRole("button", { name: "Send" }).click();
    },
    shows: (page) => page.getByText("I'm checking"),
  },
  {
    name: "assistant-approval",
    route: "/assistant?conversation=conversation-awaiting",
    handlers: conversationsOf([awaitingApproval, untitled, roles]),
    shows: (page) => page.getByRole("button", { name: "Approve" }),
  },
  {
    name: "assistant-error",
    route: "/assistant?conversation=conversation-failed",
    handlers: conversationsOf([failed, untitled, roles]),
    shows: (page) => page.getByText(/couldn't answer/),
  },
];
