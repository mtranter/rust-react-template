import type { ConversationResponse, MessageResponse, ToolCallResponse } from "@/api";
import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";
import { assistantApi, scriptedTurn } from "./assistant-fixtures";

const SUMMARY = "Add an item to a note's checklist.";

function addMilk(): ToolCallResponse {
  return {
    id: "call-1",
    tool: "add_note_item",
    summary: SUMMARY,
    arguments: { id: "note-1", body: { text: "Milk" } },
    status: "awaiting_approval",
  };
}

/** Alice asked for milk on her list, and the assistant waits for her to approve adding it. */
function awaitingApproval(): ConversationResponse {
  return {
    id: "conversation-milk",
    title: "Add milk to my list",
    createdAt: "2026-01-02T09:00:00Z",
    messages: [
      {
        kind: "user",
        id: "message-1",
        postedAt: "2026-01-02T09:00:00Z",
        text: "Add milk to my list",
      },
      {
        kind: "toolCalls",
        id: "message-2",
        postedAt: "2026-01-02T09:00:01Z",
        toolCalls: [addMilk()],
      },
    ],
    version: "3",
  };
}

function storedCall(conversation: ConversationResponse) {
  const message = conversation.messages[1];
  if (message.kind !== "toolCalls") throw new Error("not a message of tool calls");
  return message.toolCalls[0];
}

function store(conversation: ConversationResponse, ...messages: MessageResponse[]) {
  conversation.messages.push(...messages);
  conversation.version = String(Number(conversation.version) + messages.length);
  return { messageIds: messages.map((m) => m.id), version: conversation.version };
}

async function open(conversation: ConversationResponse) {
  const user = userEvent.setup();
  renderAt(`/assistant?conversation=${conversation.id}`);
  await screen.findByRole("article", { name: conversation.title || "New conversation" });
  return { user, opened: () => within(screen.getByRole("article")) };
}

test("a pending write is a card that waits while the turn runs, then can be decided", async () => {
  const conversation: ConversationResponse = {
    id: "conversation-blank",
    title: "",
    createdAt: "2026-01-02T09:00:00Z",
    messages: [],
    version: "1",
  };
  const turn = scriptedTurn();
  server.use(...assistantApi([conversation], [turn]).handlers);
  const { user, opened } = await open(conversation);

  await user.type(opened().getByRole("textbox", { name: "Message" }), "Add milk{Enter}");
  const { id, tool, summary, arguments: args } = addMilk();
  turn.emit({ kind: "approvalRequired", id, tool, summary: summary ?? "", arguments: args });

  const live = await opened().findByRole("region", { name: "Approve add_note_item?" });
  expect(live).toHaveTextContent(SUMMARY);
  expect(live).toHaveTextContent(/"text": "Milk"/);
  expect(within(live).getByRole("button", { name: "Approve" })).toBeDisabled();
  expect(within(live).getByRole("button", { name: "Reject" })).toBeDisabled();

  turn.emit({
    kind: "finished",
    ...store(conversation, {
      kind: "toolCalls",
      id: "message-2",
      postedAt: "2026-01-02T09:00:01Z",
      toolCalls: [addMilk()],
    }),
  });
  turn.end();

  await waitFor(() => expect(opened().getByRole("button", { name: "Approve" })).toBeEnabled());
  expect(opened().getByRole("button", { name: "Reject" })).toBeEnabled();
  expect(opened().getByRole("region", { name: "Approve add_note_item?" })).toHaveTextContent(
    SUMMARY,
  );
});

test("approving runs the write, streams the resumed turn and refreshes every screen", async () => {
  const conversation = awaitingApproval();
  const turn = scriptedTurn();
  const api = assistantApi([conversation], [turn]);
  let statusAsked = 0;
  server.use(
    http.get("*/api/assistant/status", () => {
      statusAsked += 1;
      return HttpResponse.json({ on: true });
    }),
    ...api.handlers,
  );
  const { user, opened } = await open(conversation);
  const card = opened().getByRole("region", { name: "Approve add_note_item?" });
  expect(card).toHaveTextContent(SUMMARY);
  expect(card).toHaveTextContent(/"text": "Milk"/);

  await user.click(within(card).getByRole("button", { name: "Approve" }));

  await waitFor(() => expect(api.decided).toEqual([{ callId: "call-1", decision: "approve" }]));
  expect(opened().getByRole("textbox", { name: "Message" })).toBeDisabled();
  turn.emit(
    { kind: "decided", id: "call-1", status: "approved" },
    { kind: "toolResult", id: "call-1", result: { items: [{ text: "Milk" }] } },
  );
  expect(await opened().findByRole("button", { name: /add_note_item/ })).toHaveTextContent(
    "Approved",
  );
  expect(
    opened().queryByRole("region", { name: "Approve add_note_item?" }),
  ).not.toBeInTheDocument();
  turn.emit({ kind: "text", text: "Milk is on your list." });
  expect(await opened().findByLabelText("Assistant")).toHaveTextContent("Milk is on your list.");

  const before = statusAsked;
  storedCall(conversation).result = { items: [{ text: "Milk" }] };
  turn.emit({
    kind: "finished",
    ...store(conversation, {
      kind: "assistant",
      id: "message-3",
      postedAt: "2026-01-02T09:00:02Z",
      text: "Milk is on your list.",
    }),
  });
  turn.end();

  await waitFor(() => expect(opened().getByRole("textbox", { name: "Message" })).toBeEnabled());
  await waitFor(() => expect(statusAsked).toBeGreaterThan(before));
  expect(opened().getByRole("button", { name: /add_note_item/ })).toHaveTextContent("Approved");
  expect(opened().getAllByLabelText("Assistant")).toHaveLength(1);
});

test("rejecting doesn't run the write and streams the resumed turn", async () => {
  const conversation = awaitingApproval();
  const turn = scriptedTurn();
  const api = assistantApi([conversation], [turn]);
  server.use(...api.handlers);
  const { user, opened } = await open(conversation);

  const card = opened().getByRole("region", { name: "Approve add_note_item?" });
  await user.click(within(card).getByRole("button", { name: "Reject" }));

  await waitFor(() => expect(api.decided).toEqual([{ callId: "call-1", decision: "reject" }]));
  turn.emit({ kind: "decided", id: "call-1", status: "rejected" });
  expect(await opened().findByRole("button", { name: /add_note_item/ })).toHaveTextContent(
    "Rejected",
  );
  turn.emit({ kind: "text", text: "All right, I left your list alone." });
  turn.emit({
    kind: "finished",
    ...store(conversation, {
      kind: "assistant",
      id: "message-3",
      postedAt: "2026-01-02T09:00:02Z",
      text: "All right, I left your list alone.",
    }),
  });
  turn.end();

  await waitFor(() => expect(opened().getByRole("textbox", { name: "Message" })).toBeEnabled());
  expect(opened().getByLabelText("Assistant")).toHaveTextContent(
    "All right, I left your list alone.",
  );
  expect(opened().getByRole("button", { name: /add_note_item/ })).toHaveTextContent("Rejected");
  expect(
    opened().queryByRole("region", { name: "Approve add_note_item?" }),
  ).not.toBeInTheDocument();
});

test("a call decided meanwhile says so and shows how it was decided", async () => {
  const conversation = awaitingApproval();
  server.use(...assistantApi([conversation]).handlers);
  const { user, opened } = await open(conversation);
  storedCall(conversation).status = "rejected";

  const card = opened().getByRole("region", { name: "Approve add_note_item?" });
  await user.click(within(card).getByRole("button", { name: "Approve" }));

  expect(await opened().findByRole("alert")).toHaveTextContent("isn't awaiting a decision");
  expect(await opened().findByRole("button", { name: /add_note_item/ })).toHaveTextContent(
    "Rejected",
  );
});
