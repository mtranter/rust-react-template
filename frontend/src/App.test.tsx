import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import type { ReactNode } from "react";
import { expect, test } from "vitest";
import { server } from "../vitest.setup";
import { App } from "./App";
import type { NoteResponse, PageNoteResponse } from "./api";

function renderApp(children: ReactNode) {
  // A fresh QueryClient per test with retries off keeps assertions deterministic.
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

function page(items: NoteResponse[]): PageNoteResponse {
  return { items, total: items.length, limit: 20, offset: 0 };
}

// Example test (spec §13): renders the real component through the real generated
// client, mocking only the network with MSW. A note created via the form appears
// once the list is refetched.
test("lists notes and shows a newly created note", async () => {
  const notes: NoteResponse[] = [
    {
      id: "1",
      author_id: "a1",
      title: "First note",
      body: "",
      created_at: "2026-01-01T00:00:00Z",
      version: "1",
    },
  ];

  server.use(
    http.get("*/api/notes", () => HttpResponse.json(page(notes))),
    http.post("*/api/notes", async ({ request }) => {
      const body = (await request.json()) as { author_id: string; title: string };
      const created: NoteResponse = {
        id: "2",
        author_id: body.author_id,
        title: body.title,
        body: "",
        created_at: "2026-01-02T00:00:00Z",
        version: "1",
      };
      notes.push(created);
      return HttpResponse.json(created);
    }),
  );

  const user = userEvent.setup();
  renderApp(<App />);

  expect(await screen.findByText("First note")).toBeInTheDocument();

  await user.type(screen.getByLabelText("author id"), "a1");
  await user.type(screen.getByLabelText("title"), "Second note");
  await user.click(screen.getByRole("button", { name: "Add note" }));

  await waitFor(() => expect(screen.getByText("Second note")).toBeInTheDocument());
});
