import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../vitest.setup";
import { createAppRouter } from "./router";

function renderAt(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

test.each(["/", "/home"])("shows the notes page at %s", async (path) => {
  server.use(
    http.get("*/api/notes", () => HttpResponse.json({ items: [] })),
    http.get("*/api/users", () => HttpResponse.json({ items: [] })),
  );

  renderAt(path);

  expect(await screen.findByRole("heading", { name: "Notes" })).toBeInTheDocument();
});

test("shows the search page, searching for what the URL says", async () => {
  const searched: (string | null)[] = [];
  server.use(
    http.get("*/api/note-search", ({ request }) => {
      searched.push(new URL(request.url).searchParams.get("q"));
      return HttpResponse.json({ items: [] });
    }),
  );

  renderAt("/search?q=alice");

  expect(await screen.findByRole("heading", { name: "Search notes" })).toBeInTheDocument();
  expect(screen.getByRole("searchbox", { name: "Search notes" })).toHaveValue("alice");
  await waitFor(() => expect(searched).toEqual(["alice"]));
});
