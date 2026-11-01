import type { ListResultNoteSummaryResponse, NoteSummaryResponse } from "@/api";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { useState } from "react";
import { expect, test } from "vitest";
import { NoteSearch } from ".";
import { server } from "../../../vitest.setup";

function Searchable() {
  const [text, setText] = useState("");
  return <NoteSearch text={text} onTextChange={setText} />;
}

function renderSearch() {
  // Retries off so a failing request fails the test immediately.
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <Searchable />
    </QueryClientProvider>,
  );
}

function page(items: NoteSummaryResponse[]): ListResultNoteSummaryResponse {
  return { items, total: items.length, limit: 20, offset: 0 };
}

const shopping: NoteSummaryResponse = {
  id: "1",
  title: "Shopping list",
  authorId: "a1",
  authorName: "Alice",
  createdAt: "2026-01-01T00:00:00Z",
};
const bike: NoteSummaryResponse = {
  id: "2",
  title: "Bike repairs",
  authorId: "b2",
  authorName: "Bob",
  createdAt: "2026-01-02T00:00:00Z",
};
const orphan: NoteSummaryResponse = {
  id: "3",
  title: "Lost note",
  authorId: "x9",
  createdAt: "2026-01-03T00:00:00Z",
};

const rows = () =>
  screen
    .getAllByRole("row")
    .slice(1)
    .map((row) => [...(row as HTMLTableRowElement).cells].map((cell) => cell.textContent));

test("shows each note with its author's name", async () => {
  server.use(http.get("*/api/note-search", () => HttpResponse.json(page([shopping, orphan]))));

  renderSearch();

  await screen.findByText("Shopping list");
  expect(rows().map(([title, author]) => [title, author])).toEqual([
    ["Shopping list", "Alice"],
    ["Lost note", "Unknown author"],
  ]);
});

test("searches on the server as the user types", async () => {
  const searched: (string | null)[] = [];
  server.use(
    http.get("*/api/note-search", ({ request }) => {
      const q = new URL(request.url).searchParams.get("q");
      searched.push(q);
      const notes = [shopping, bike].filter(
        (n) => !q || `${n.title} ${n.authorName}`.toLowerCase().includes(q.toLowerCase()),
      );
      return HttpResponse.json(page(notes));
    }),
  );

  const user = userEvent.setup();
  renderSearch();
  await screen.findByText("Bike repairs");

  await user.type(screen.getByRole("searchbox", { name: "Search notes" }), "bob");

  await waitFor(() => expect(screen.queryByText("Shopping list")).not.toBeInTheDocument());
  expect(screen.getByText("Bike repairs")).toBeInTheDocument();
  expect(searched[0]).toBeNull();
  expect(searched.at(-1)).toBe("bob");
});

test("says so when nothing matches", async () => {
  server.use(
    http.get("*/api/note-search", ({ request }) =>
      HttpResponse.json(page(new URL(request.url).searchParams.has("q") ? [] : [shopping])),
    ),
  );

  const user = userEvent.setup();
  renderSearch();
  await screen.findByText("Shopping list");

  await user.type(screen.getByRole("searchbox", { name: "Search notes" }), "zebra");

  expect(await screen.findByText("No notes match.")).toBeInTheDocument();
});

test("sorts by author on the server when the Author header is clicked", async () => {
  const requested: string[][] = [];
  server.use(
    http.get("*/api/note-search", ({ request }) => {
      requested.push(new URL(request.url).searchParams.getAll("sort"));
      return HttpResponse.json(page([shopping, bike]));
    }),
  );

  const user = userEvent.setup();
  renderSearch();
  await screen.findByText("Shopping list");

  await user.click(screen.getByRole("button", { name: "Author" }));

  await waitFor(() => expect(requested).toEqual([[], ["authorName"]]));
});
