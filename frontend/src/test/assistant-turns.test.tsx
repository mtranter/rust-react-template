import type { ConversationResponse, MessageResponse } from "@/api";
import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";
import { assistantApi, rolesConversation, scriptedTurn } from "./assistant-fixtures";

function blank(): ConversationResponse {
  return {
    id: "conversation-blank",
    title: "",
    createdAt: "2026-01-02T09:00:00Z",
    messages: [],
    version: "1",
  };
}

/** Stores `messages` in `conversation` as the server does once a turn has posted them. */
function store(conversation: ConversationResponse, ...messages: MessageResponse[]) {
  conversation.messages.push(...messages);
  conversation.version = String(Number(conversation.version) + messages.length);
  return { messageIds: messages.map((m) => m.id), version: conversation.version };
}

async function openBlank(conversation: ConversationResponse) {
  const user = userEvent.setup();
  renderAt(`/assistant?conversation=${conversation.id}`);
  await screen.findByRole("article", { name: "New conversation" });
  return { user, opened: () => within(screen.getByRole("article")) };
}

test("streams a read's answer in as it arrives, then shows the turn as stored", async () => {
  const conversation = blank();
  const turn = scriptedTurn();
  const api = assistantApi([conversation], [turn]);
  server.use(...api.handlers);
  const { user, opened } = await openBlank(conversation);

  await user.type(opened().getByRole("textbox", { name: "Message" }), "Which roles are there?");
  await user.click(opened().getByRole("button", { name: "Send" }));

  expect(await opened().findByLabelText("You")).toHaveTextContent("Which roles are there?");
  expect(opened().getByRole("textbox", { name: "Message" })).toBeDisabled();
  expect(opened().getByRole("textbox", { name: "Message" })).toHaveValue("");
  expect(api.sent).toEqual([{ text: "Which roles are there?", ifMatch: '"1"' }]);

  turn.emit({
    kind: "toolCallStarted",
    id: "call-1",
    tool: "list_roles",
    arguments: { limit: 20 },
  });
  expect(await opened().findByRole("button", { name: /list_roles/ })).toHaveTextContent("Running");

  turn.emit({
    kind: "toolResult",
    id: "call-1",
    result: { items: [{ name: "admin" }, { name: "user" }] },
  });
  await waitFor(() =>
    expect(opened().getByRole("button", { name: /list_roles/ })).toHaveTextContent("Ran"),
  );

  turn.emit({ kind: "text", text: "There are " });
  expect(await opened().findByLabelText("Assistant")).toHaveTextContent("There are");
  turn.emit({ kind: "text", text: "**two** roles." });
  await waitFor(() =>
    expect(opened().getByLabelText("Assistant")).toHaveTextContent("There are two roles."),
  );
  expect(within(opened().getByLabelText("Assistant")).getByText("two").tagName).toBe("STRONG");
  expect(opened().getByRole("textbox", { name: "Message" })).toBeDisabled();

  turn.emit({
    kind: "finished",
    ...store(
      conversation,
      {
        kind: "toolCalls",
        id: "message-2",
        postedAt: "2026-01-03T09:00:01Z",
        toolCalls: [
          {
            id: "call-1",
            tool: "list_roles",
            arguments: { limit: 20 },
            status: "ran",
            result: { items: [{ name: "admin" }, { name: "user" }] },
          },
        ],
      },
      {
        kind: "assistant",
        id: "message-3",
        postedAt: "2026-01-03T09:00:02Z",
        text: "There are **two** roles.",
      },
    ),
  });
  turn.end();

  await waitFor(() => expect(opened().getByRole("textbox", { name: "Message" })).toBeEnabled());
  expect(screen.getByRole("article", { name: "Which roles are there?" })).toBeInTheDocument();
  expect(opened().getAllByLabelText("Assistant")).toHaveLength(1);
  expect(opened().getAllByRole("button", { name: /list_roles/ })).toHaveLength(1);
  expect(opened().queryByLabelText("Working")).not.toBeInTheDocument();

  await user.click(opened().getByRole("button", { name: /list_roles/ }));
  expect(await opened().findByText(/"limit": 20/)).toBeInTheDocument();
  expect(opened().getByText(/"name": "admin"/)).toBeInTheDocument();
});

test("shows an error mid-stream inline, then the error the turn stored", async () => {
  const conversation = blank();
  const turn = scriptedTurn();
  server.use(...assistantApi([conversation], [turn]).handlers);
  const { user, opened } = await openBlank(conversation);

  await user.type(opened().getByRole("textbox", { name: "Message" }), "Hello{Enter}");
  turn.emit(
    { kind: "text", text: "Let me" },
    {
      kind: "error",
      error: { code: "unavailable", message: "the model is throttled", requestId: "test" },
    },
  );

  expect(await opened().findByRole("alert")).toHaveTextContent("the model is throttled");
  expect(opened().getByLabelText("Assistant")).toHaveTextContent("Let me");

  turn.emit({
    kind: "finished",
    ...store(conversation, {
      kind: "error",
      id: "message-2",
      postedAt: "2026-01-03T09:00:01Z",
      text: "The assistant couldn't answer: the model is throttled.",
    }),
  });
  turn.end();

  expect(await opened().findByLabelText("Error")).toHaveTextContent(
    "The assistant couldn't answer: the model is throttled.",
  );
  expect(opened().queryByRole("alert")).not.toBeInTheDocument();
  expect(opened().getByRole("textbox", { name: "Message" })).toBeEnabled();
});

test("says so when the stream ends before the turn does", async () => {
  const conversation = blank();
  const turn = scriptedTurn();
  server.use(...assistantApi([conversation], [turn]).handlers);
  const { user, opened } = await openBlank(conversation);

  await user.type(opened().getByRole("textbox", { name: "Message" }), "Hello{Enter}");
  turn.emit({ kind: "text", text: "Let me" });
  turn.end();

  expect(await opened().findByRole("alert")).toHaveTextContent(/connection .* was lost/);
  expect(opened().getByLabelText("You")).toHaveTextContent("Hello");
  expect(opened().getByRole("textbox", { name: "Message" })).toBeEnabled();
});

test("a message refused before its turn is kept to send again, saying why", async () => {
  const conversation = blank();
  server.use(
    http.post("*/api/assistant/conversations/:id/messages", () =>
      HttpResponse.json(
        { code: "unavailable", message: "the assistant is off", requestId: "test" },
        { status: 503 },
      ),
    ),
    ...assistantApi([conversation]).handlers,
  );
  const { user, opened } = await openBlank(conversation);

  await user.type(opened().getByRole("textbox", { name: "Message" }), "Hello{Enter}");

  expect(await opened().findByRole("alert")).toHaveTextContent("the assistant is off");
  expect(opened().getByRole("textbox", { name: "Message" })).toHaveValue("Hello");
  expect(opened().queryByLabelText("You")).not.toBeInTheDocument();
});

test("a conversation changed meanwhile is reloaded, and sent to at its new version", async () => {
  const conversation = blank();
  const turn = scriptedTurn();
  const api = assistantApi([conversation], [turn]);
  server.use(...api.handlers);
  const { user, opened } = await openBlank(conversation);
  conversation.version = "2";

  await user.type(opened().getByRole("textbox", { name: "Message" }), "Hello{Enter}");
  expect(await opened().findByRole("alert")).toHaveTextContent(/Someone else changed this/);
  await user.click(opened().getByRole("button", { name: "Send" }));
  turn.emit({ kind: "finished", messageIds: [], version: "3" });
  turn.end();

  await waitFor(() => expect(api.sent.map((s) => s.ifMatch)).toEqual(['"1"', '"2"']));
});

test("renders the assistant's markdown without the HTML in it", async () => {
  const conversation: ConversationResponse = {
    ...rolesConversation,
    messages: [
      {
        kind: "assistant",
        id: "message-1",
        postedAt: "2026-01-01T09:00:00Z",
        text: 'Done. <img src="x" onerror="alert(1)"><script>alert(2)</script> [link](javascript:alert(3))',
      },
    ],
  };
  server.use(...assistantApi([conversation]).handlers);
  renderAt(`/assistant?conversation=${conversation.id}`);

  const said = await screen.findByLabelText("Assistant");
  expect(said).toHaveTextContent("Done.");
  expect(said.querySelector("img, script")).toBeNull();
  expect(within(said).getByText("link").getAttribute("href") ?? "").not.toMatch(/javascript/);
});

test("a turn whose write ran refreshes every screen's data", async () => {
  const conversation = blank();
  const turn = scriptedTurn();
  let statusAsked = 0;
  server.use(
    http.get("*/api/assistant/status", () => {
      statusAsked += 1;
      return HttpResponse.json({ on: true });
    }),
    ...assistantApi([conversation], [turn]).handlers,
  );
  const { user, opened } = await openBlank(conversation);
  const before = statusAsked;

  await user.type(opened().getByRole("textbox", { name: "Message" }), "Rename it{Enter}");
  turn.emit({
    kind: "finished",
    ...store(conversation, {
      kind: "toolCalls",
      id: "message-2",
      postedAt: "2026-01-03T09:00:01Z",
      toolCalls: [
        {
          id: "call-1",
          tool: "rename_role",
          arguments: { name: "admin" },
          status: "approved",
          result: { name: "owner" },
        },
      ],
    }),
  });
  turn.end();

  await waitFor(() => expect(statusAsked).toBeGreaterThan(before));
});
