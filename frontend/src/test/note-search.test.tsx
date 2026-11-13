import type { ListResultNoteSummaryResponse } from "@/api";
import { listRequests } from "@/test/fixtures";
import { renderAt } from "@/test/render-app";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

const found: ListResultNoteSummaryResponse = {
  items: Array.from({ length: 25 }, (_, i) => ({
    id: `n${i}`,
    title: `Found ${i}`,
    authorId: "a1",
    authorName: "Alice",
    createdAt: "2026-01-01T00:00:00Z",
  })),
  total: 25,
  limit: 20,
  offset: 0,
};

test("a search link keeps its text, page and sort", async () => {
  const list = listRequests("*/api/note-search");
  server.use(
    list.handler,
    http.get("*/api/note-search", () => HttpResponse.json(found)),
  );

  renderAt("/search?q=alice&page=2&sort=authorName");

  expect(await screen.findByText("Found 0")).toBeInTheDocument();
  expect(list.requests).toContainEqual({ offset: "20", sort: "authorName" });
});

test("searching again starts from the first page", async () => {
  server.use(http.get("*/api/note-search", () => HttpResponse.json(found)));
  const user = userEvent.setup();
  const { router } = renderAt("/search?page=2");
  await screen.findByText("Found 0");

  await user.type(screen.getByRole("searchbox", { name: "Search notes" }), "b");

  await waitFor(() => expect(router.state.location.href).toBe("/search?q=b"));
});
