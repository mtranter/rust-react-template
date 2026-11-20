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
    arguments: { id: "note-1", idempotency_key: "key-1", body: { text: "Milk" } },
    status: "awaiting_approval",
    proposal: {
      action: "Add note item",
      target: { kind: "Note", name: "Shopping list" },
      fields: [{ label: "Text", value: "Milk" }],
    },
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
  const { id, tool, summary, arguments: args, proposal } = addMilk();
  turn.emit({
    kind: "approvalRequired",
    id,
    tool,
    summary: summary ?? "",
    arguments: args,
    proposal,
  });

  const live = await opened().findByRole("region", { name: "Add note item: Shopping list" });
  expect(live).toHaveTextContent("The assistant wants to do this for you");
  expect(within(live).getByRole("definition")).toHaveTextContent("Milk");
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
  expect(
    opened().getByRole("region", { name: "Add note item: Shopping list" }),
  ).toBeInTheDocument();
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
  const card = opened().getByRole("region", { name: "Add note item: Shopping list" });
  expect(within(card).getByRole("definition")).toHaveTextContent("Milk");

  await user.click(within(card).getByRole("button", { name: "Approve" }));

  await waitFor(() => expect(api.decided).toEqual([{ callId: "call-1", decision: "approve" }]));
  expect(opened().getByRole("textbox", { name: "Message" })).toBeDisabled();
  turn.emit(
    { kind: "decided", id: "call-1", status: "approved" },
    { kind: "toolResult", id: "call-1", result: { items: [{ text: "Milk" }] } },
  );
  expect(await opened().findByRole("button", { name: /Add note item/ })).toHaveTextContent(
    "Approved",
  );
  expect(
    opened().queryByRole("region", { name: "Add note item: Shopping list" }),
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
  expect(opened().getByRole("button", { name: /Add note item/ })).toHaveTextContent("Approved");
  expect(opened().getAllByLabelText("Assistant")).toHaveLength(1);
});

test("rejecting doesn't run the write and streams the resumed turn", async () => {
  const conversation = awaitingApproval();
  const turn = scriptedTurn();
  const api = assistantApi([conversation], [turn]);
  server.use(...api.handlers);
  const { user, opened } = await open(conversation);

  const card = opened().getByRole("region", { name: "Add note item: Shopping list" });
  await user.click(within(card).getByRole("button", { name: "Reject" }));

  await waitFor(() => expect(api.decided).toEqual([{ callId: "call-1", decision: "reject" }]));
  turn.emit({ kind: "decided", id: "call-1", status: "rejected" });
  expect(await opened().findByRole("button", { name: /Add note item/ })).toHaveTextContent(
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
  expect(opened().getByRole("button", { name: /Add note item/ })).toHaveTextContent("Rejected");
  expect(
    opened().queryByRole("region", { name: "Add note item: Shopping list" }),
  ).not.toBeInTheDocument();
});

test("a call decided meanwhile says so and shows how it was decided", async () => {
  const conversation = awaitingApproval();
  server.use(...assistantApi([conversation]).handlers);
  const { user, opened } = await open(conversation);
  storedCall(conversation).status = "rejected";

  const card = opened().getByRole("region", { name: "Add note item: Shopping list" });
  await user.click(within(card).getByRole("button", { name: "Approve" }));

  expect(await opened().findByRole("alert")).toHaveTextContent("isn't awaiting a decision");
  expect(await opened().findByRole("button", { name: /Add note item/ })).toHaveTextContent(
    "Rejected",
  );
});

const EDIT_ID = "call-7f3a9c2e";

/** The assistant asks to rename Alice's list and set when it archives, the arguments as an agent sends them. */
function editList(): ToolCallResponse {
  return {
    id: EDIT_ID,
    tool: "edit_note",
    summary: "Replace a note. Requires `If-Match`; a stale one is refused with `412`.",
    arguments: {
      id: "0b6f1c1e-5d4a-4f1e-9a57-2f1e7c3f2d11",
      expected_version: "4",
      idempotency_key: "key-from-the-model",
      body: { title: "Groceries", archiveAt: "2026-12-24", pinned: true, tags: ["food", "weekly"] },
    },
    status: "awaiting_approval",
    proposal: {
      action: "Edit note",
      target: { kind: "Note", name: "Shopping list" },
      fields: [
        { label: "Title", value: "Groceries", previous: "Shopping list" },
        { label: "Archive at", value: "2026-12-24", format: "date" },
        { label: "Pinned", value: true },
        { label: "Tags", value: ["food", "weekly"] },
        { label: "Body", value: null },
      ],
    },
  };
}

/** What the card shows: each label with its value, as the user reads them. */
function described(card: HTMLElement) {
  const labels = within(card)
    .getAllByRole("term")
    .map((term) => term.textContent);
  const values = within(card).getAllByRole("definition");
  return Object.fromEntries(labels.map((label, at) => [label, values[at]]));
}

function expectDescribesTheEdit(card: HTMLElement) {
  expect(card).toHaveTextContent("Edit note: Shopping list");
  expect(card).toHaveTextContent("The assistant wants to do this for you");
  const fields = described(card);
  expect(Object.keys(fields)).toEqual(["Title", "Archive at", "Pinned", "Tags", "Body"]);
  expect(within(fields.Title).getByText("Shopping list").tagName).toBe("DEL");
  expect(fields.Title).toHaveTextContent("Shopping list → Groceries");
  const christmasEve = new Intl.DateTimeFormat(undefined, { dateStyle: "long" }).format(
    new Date(2026, 11, 24),
  );
  expect(fields["Archive at"]).toHaveTextContent(christmasEve);
  expect(fields.Pinned).toHaveTextContent("Yes");
  expect(fields.Tags).toHaveTextContent("food, weekly");
  expect(fields.Body).toHaveTextContent("None");
  const text = card.textContent ?? "";
  for (const hidden of [
    "edit_note",
    "expected_version",
    "idempotency_key",
    "key-from-the-model",
    EDIT_ID,
    "0b6f1c1e",
    "If-Match",
    "{",
  ]) {
    expect(text).not.toContain(hidden);
  }
}

test("a pending write streamed in is described in plain words, with no ids or JSON", async () => {
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

  await user.type(opened().getByRole("textbox", { name: "Message" }), "Rename my list{Enter}");
  const { id, tool, summary, arguments: args, proposal } = editList();
  turn.emit({ kind: "toolCallStarted", id, tool, summary, arguments: args });
  turn.emit({
    kind: "approvalRequired",
    id,
    tool,
    summary: summary ?? "",
    arguments: args,
    proposal,
  });

  expectDescribesTheEdit(await opened().findByRole("region", { name: "Edit note: Shopping list" }));
  turn.end();
});

test("a pending write is described the same when the conversation is reopened", async () => {
  const conversation = awaitingApproval();
  const message = conversation.messages[1];
  if (message.kind === "toolCalls") message.toolCalls = [editList()];
  server.use(...assistantApi([conversation]).handlers);
  const { opened } = await open(conversation);

  const card = opened().getByRole("region", { name: "Edit note: Shopping list" });
  expectDescribesTheEdit(card);
  expect(within(card).getByRole("button", { name: "Approve" })).toBeEnabled();
});

test("a long value is clamped until shown in full", async () => {
  const conversation = awaitingApproval();
  const call = storedCall(conversation);
  const long = "Milk, eggs, bread, butter, cheese, apples, ".repeat(6);
  call.proposal = { action: "Add note item", fields: [{ label: "Text", value: long }] };
  server.use(...assistantApi([conversation]).handlers);
  const { user, opened } = await open(conversation);

  const card = opened().getByRole("region", { name: "Add note item" });
  await user.click(within(card).getByRole("button", { name: "Show more" }));
  expect(within(card).getByRole("button", { name: "Show less" })).toBeInTheDocument();
});

test("a pending write the app can no longer describe can only be rejected", async () => {
  const conversation = awaitingApproval();
  storedCall(conversation).proposal = undefined;
  server.use(...assistantApi([conversation]).handlers);
  const { opened } = await open(conversation);

  const card = opened().getByRole("region", { name: "The assistant wants to make a change" });
  expect(card).not.toHaveTextContent("add_note_item");
  expect(within(card).getByRole("button", { name: "Approve" })).toBeDisabled();
  expect(within(card).getByRole("button", { name: "Reject" })).toBeEnabled();
});
