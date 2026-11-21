import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";
import { assistantApi, rolesConversation, untitledConversation } from "./assistant-fixtures";

function conversations() {
  return within(screen.getByRole("navigation", { name: "Conversations" }));
}

/** The text of each conversation listed, in order. */
function listed() {
  return conversations()
    .queryAllByRole("listitem")
    .map((item) => item.textContent);
}

test("links to the assistant from the header when it is on", async () => {
  server.use(...assistantApi([]).handlers);
  const user = userEvent.setup();
  const { router } = renderAt("/access");

  await user.click(await screen.findByRole("link", { name: "Assistant" }));

  expect(await screen.findByRole("heading", { name: "Assistant" })).toBeInTheDocument();
  expect(screen.getByRole("region", { name: "Chat" })).toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/assistant");
});

test("hides the assistant's link while it is off", async () => {
  renderAt("/access");

  expect(await screen.findByText(/Only an admin can manage/)).toBeInTheDocument();
  expect(screen.queryByRole("link", { name: "Assistant" })).not.toBeInTheDocument();
});

test("hides the assistant's page while it is off, even linked to it", async () => {
  renderAt("/assistant");

  expect(await screen.findByText("The assistant isn't available.")).toBeInTheDocument();
  expect(screen.queryByRole("region", { name: "Chat" })).not.toBeInTheDocument();
  expect(screen.queryByRole("link", { name: "Assistant" })).not.toBeInTheDocument();
});

test("treats a caller who may not chat as the assistant off", async () => {
  server.use(
    http.get("*/api/assistant/status", () =>
      HttpResponse.json(
        { code: "forbidden", message: "forbidden", requestId: "test" },
        { status: 403 },
      ),
    ),
  );
  renderAt("/assistant");

  expect(await screen.findByText("The assistant isn't available.")).toBeInTheDocument();
  expect(screen.queryByRole("region", { name: "Chat" })).not.toBeInTheDocument();
  expect(screen.queryByRole("link", { name: "Assistant" })).not.toBeInTheDocument();
});

test("lists the caller's conversations newest first, an untitled one as new", async () => {
  server.use(...assistantApi([rolesConversation, untitledConversation]).handlers);
  renderAt("/assistant");

  await screen.findByRole("button", { name: /^Which roles are there\?/ });
  expect(listed()).toEqual([
    expect.stringMatching(/^New conversation/),
    expect.stringMatching(/^Which roles are there\?/),
  ]);
});

test("says so when the caller has no conversations yet", async () => {
  server.use(...assistantApi([]).handlers);
  renderAt("/assistant");

  expect(await screen.findByText("No conversations yet.")).toBeInTheDocument();
});

test("starts a conversation and opens it", async () => {
  server.use(...assistantApi([rolesConversation]).handlers);
  const user = userEvent.setup();
  const { router } = renderAt("/assistant");
  await screen.findByRole("button", { name: /^Which roles are there\?/ });

  await user.click(screen.getByRole("button", { name: "New conversation" }));

  const opened = await screen.findByRole("article", { name: "New conversation" });
  expect(within(opened).getByText("No messages yet.")).toBeInTheDocument();
  expect(router.state.location.search).toEqual({ conversation: "conversation-started-1" });
  expect(
    await conversations().findByRole("button", { name: /^New conversation/, current: "page" }),
  ).toBeInTheDocument();
});

test("opens a conversation, showing its messages and each tool call", async () => {
  server.use(...assistantApi([rolesConversation]).handlers);
  const user = userEvent.setup();
  const { router } = renderAt("/assistant");

  await user.click(await screen.findByRole("button", { name: /^Which roles are there\?/ }));

  const opened = await screen.findByRole("article", { name: "Which roles are there?" });
  expect(within(opened).getByLabelText("You")).toHaveTextContent("Which roles are there?");
  expect(within(opened).getByLabelText("Assistant")).toHaveTextContent(
    "There are two roles: admin and user.",
  );
  expect(router.state.location.search).toEqual({ conversation: "conversation-roles" });

  const lookup = within(opened).getByRole("button", { name: /List roles/ });
  expect(lookup).toHaveTextContent("Ran");
  expect(lookup).not.toHaveTextContent("list_roles");
  expect(within(opened).getByText("List the roles there are.")).toBeInTheDocument();
  await user.click(lookup);
  expect(await within(opened).findByText("Result")).toBeInTheDocument();
  expect(within(opened).getByText(/"name": "admin"/)).toBeInTheDocument();

  const declined = within(opened).getByRole("button", { name: /Delete role/ });
  expect(declined).toHaveTextContent("Rejected");
  await user.click(declined);
  expect(await within(opened).findByText(/the user declined this action/)).toBeInTheDocument();
});

test("opens the conversation a link names", async () => {
  server.use(...assistantApi([rolesConversation]).handlers);
  renderAt("/assistant?conversation=conversation-roles");

  expect(
    await screen.findByRole("article", { name: "Which roles are there?" }),
  ).toBeInTheDocument();
});

test("says so when a linked conversation isn't there", async () => {
  server.use(...assistantApi([]).handlers);
  renderAt("/assistant?conversation=gone");

  expect(await screen.findByText("This conversation isn't there any more.")).toBeInTheDocument();
});

test("deletes a conversation at the version listed, closing it if open", async () => {
  const api = assistantApi([rolesConversation, untitledConversation]);
  server.use(...api.handlers);
  const user = userEvent.setup();
  const { router } = renderAt("/assistant?conversation=conversation-roles");
  await screen.findByRole("article", { name: "Which roles are there?" });

  await user.click(screen.getByRole("button", { name: "Delete Which roles are there?" }));

  await waitFor(() =>
    expect(
      conversations().queryByRole("button", { name: /^Which roles are there\?/ }),
    ).not.toBeInTheDocument(),
  );
  expect(screen.queryByRole("article")).not.toBeInTheDocument();
  expect(router.state.location.search).toEqual({});
  expect(api.ifMatches).toEqual(['"3"']);
  expect(listed()).toEqual([expect.stringMatching(/^New conversation/)]);
});

test("a conversation changed meanwhile isn't deleted, and says so", async () => {
  const stale = { ...rolesConversation };
  const api = assistantApi([stale]);
  server.use(...api.handlers);
  const user = userEvent.setup();
  renderAt("/assistant");
  await screen.findByRole("button", { name: /^Which roles are there\?/ });
  stale.version = "4";

  await user.click(screen.getByRole("button", { name: "Delete Which roles are there?" }));

  expect(await screen.findByText(/Someone else changed this conversation/)).toBeInTheDocument();
  expect(api.ifMatches).toEqual(['"3"']);

  await user.click(screen.getByRole("button", { name: "Delete Which roles are there?" }));
  await waitFor(() => expect(api.ifMatches).toEqual(['"3"', '"4"']));
  await waitFor(() =>
    expect(
      screen.queryByRole("button", { name: /^Which roles are there\?/ }),
    ).not.toBeInTheDocument(),
  );
});
