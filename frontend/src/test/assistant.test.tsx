import { renderAt } from "@/test/render-app";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

const assistantOn = http.get("*/api/assistant/status", () => HttpResponse.json({ on: true }));

test("links to the assistant from the header when it is on", async () => {
  server.use(assistantOn);
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
