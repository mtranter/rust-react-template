import type { ListResultNoteSummaryResponse } from "@/api";
import { listRequests } from "@/test/fixtures";
import { holding } from "@/test/may-use-everything";
import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
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

  renderAt("/notes/search?q=alice&page=2&sort=authorName");

  expect(await screen.findByText("Found 0")).toBeInTheDocument();
  expect(list.requests).toContainEqual({ offset: "20", sort: "authorName" });
});

test("searching again starts from the first page", async () => {
  server.use(http.get("*/api/note-search", () => HttpResponse.json(found)));
  const user = userEvent.setup();
  const { router } = renderAt("/notes/search?page=2");
  await screen.findByText("Found 0");

  await user.type(screen.getByRole("searchbox", { name: "Search notes" }), "b");

  await waitFor(() => expect(router.state.location.href).toBe("/notes/search?q=b"));
});

test("a caller who may only search sees Notes with Search alone", async () => {
  server.use(
    holding([{ resource: "noteSummary", action: "search" }]),
    http.get("*/api/note-search", () => HttpResponse.json(found)),
  );
  renderAt("/notes/search");

  const areas = await screen.findByRole("navigation", { name: "Notes" });
  expect(within(areas).getByRole("link", { name: "Search" })).toBeInTheDocument();
  expect(within(areas).queryByRole("link", { name: "All notes" })).not.toBeInTheDocument();
});
