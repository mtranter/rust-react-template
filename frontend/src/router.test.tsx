import { screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../vitest.setup";
import { renderAt } from "./test/render-app";

test("shows the launcher at /, listing the apps", async () => {
  renderAt("/");

  const apps = await screen.findByRole("region", { name: "Apps" });
  expect(apps).toHaveTextContent("Notes");
});

test.each(["/notes", "/home"])("shows the notes page at %s", async (path) => {
  server.use(http.get("*/api/notes", () => HttpResponse.json({ items: [] })));

  renderAt(path);

  expect(await screen.findByRole("heading", { name: "Notes" })).toBeInTheDocument();
});

test.each(["/notes/search?q=alice", "/search?q=alice"])(
  "shows the search page at %s, searching for what the URL says",
  async (path) => {
    const searched: (string | null)[] = [];
    server.use(
      http.get("*/api/note-search", ({ request }) => {
        searched.push(new URL(request.url).searchParams.get("q"));
        return HttpResponse.json({ items: [] });
      }),
    );

    const { router } = renderAt(path);

    expect(await screen.findByRole("heading", { name: "Search notes" })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: "Search notes" })).toHaveValue("alice");
    await waitFor(() => expect(searched).toEqual(["alice"]));
    expect(router.state.location.pathname).toBe("/notes/search");
  },
);
