import type { ErrorBody, ListResultNoteResponse, NoteResponse } from "@/api";
import { Toaster } from "@/components/ui/toast";
import { createQueryClient } from "@/lib/query-client";
import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import type { ReactNode } from "react";
import { expect, test } from "vitest";
import { NewNoteForm, NotesTable } from ".";
import { server } from "../../../vitest.setup";

function renderWithQuery(children: ReactNode) {
  // Retries off so a failing request fails the test immediately.
  const queryClient = createQueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <Toaster>{children}</Toaster>
    </QueryClientProvider>,
  );
}

function page(items: NoteResponse[]): ListResultNoteResponse {
  return { items, total: items.length, limit: 20, offset: 0 };
}

test("lists notes and shows a newly created note", async () => {
  const notes: NoteResponse[] = [
    {
      id: "1",
      authorId: "a1",
      title: "First note",
      body: "",
      createdAt: "2026-01-01T00:00:00Z",
      version: "1",
    },
  ];

  const posted: unknown[] = [];
  server.use(
    http.get("*/api/users", () =>
      HttpResponse.json({
        items: [
          {
            id: "a1",
            email: "alice@example.com",
            displayName: "Alice",
            createdAt: "",
            version: "1",
          },
          { id: "b2", email: "bob@example.com", displayName: "Bob", createdAt: "", version: "1" },
        ],
      }),
    ),
    http.get("*/api/notes", () => HttpResponse.json(page(notes))),
    http.post("*/api/notes", async ({ request }) => {
      const body = (await request.json()) as { authorId: string; title: string };
      posted.push(body);
      const created: NoteResponse = {
        id: "2",
        authorId: body.authorId,
        title: body.title,
        body: "",
        createdAt: "2026-01-02T00:00:00Z",
        version: "1",
      };
      notes.push(created);
      return HttpResponse.json(created);
    }),
  );

  const user = userEvent.setup();
  renderWithQuery(
    <>
      <NewNoteForm />
      <NotesTable />
    </>,
  );

  expect(await screen.findByText("First note")).toBeInTheDocument();

  await user.click(screen.getByRole("combobox", { name: "Author" }));
  await user.click(await screen.findByRole("option", { name: "Bob" }));
  await user.type(screen.getByLabelText("Title"), "Second note");
  await user.click(screen.getByRole("button", { name: "Add note" }));

  await waitFor(() => expect(screen.getByText("Second note")).toBeInTheDocument());
  expect(posted).toEqual([{ authorId: "b2", title: "Second note" }]);
});

const authors = {
  items: [
    { id: "a1", email: "alice@example.com", displayName: "Alice", createdAt: "", version: "1" },
  ],
};

async function submitNote(title: string) {
  const user = userEvent.setup();
  await user.click(screen.getByRole("combobox", { name: "Author" }));
  await user.click(await screen.findByRole("option", { name: "Alice" }));
  await user.clear(screen.getByLabelText("Title"));
  await user.type(screen.getByLabelText("Title"), title);
  await user.click(screen.getByRole("button", { name: "Add note" }));
  return user;
}

test("shows a refused title beside the Title field, and saves once it's fixed", async () => {
  const tooLong: ErrorBody = {
    code: "validation_error",
    message: "title must be at most 200 characters",
    fields: [{ path: "title", message: "title must be at most 200 characters" }],
    requestId: "r1",
  };
  const posted: string[] = [];
  server.use(
    http.get("*/api/users", () => HttpResponse.json(authors)),
    http.post("*/api/notes", async ({ request }) => {
      const { title } = (await request.json()) as { title: string };
      posted.push(title);
      if (title.length > 3) return HttpResponse.json(tooLong, { status: 400 });
      return HttpResponse.json({ ...note(1), title });
    }),
  );

  renderWithQuery(<NewNoteForm />);
  await submitNote("Long");

  const title = screen.getByLabelText("Title");
  expect(await screen.findByText("title must be at most 200 characters")).toBeInTheDocument();
  expect(title).toHaveAttribute("aria-invalid", "true");
  expect(title).toHaveAccessibleDescription("title must be at most 200 characters");
  // Placed on the field, so not toasted as well.
  expect(screen.getAllByText("title must be at most 200 characters")).toHaveLength(1);

  const user = userEvent.setup();
  await user.clear(title);
  await user.type(title, "Ok");
  await user.click(screen.getByRole("button", { name: "Add note" }));

  await waitFor(() => expect(title).toHaveValue(""));
  expect(screen.queryByText("title must be at most 200 characters")).not.toBeInTheDocument();
  expect(posted).toEqual(["Long", "Ok"]);
});

test("toasts a refusal that names no field", async () => {
  const refused: ErrorBody = { code: "internal_error", message: "internal error", requestId: "r2" };
  server.use(
    http.get("*/api/users", () => HttpResponse.json(authors)),
    http.post("*/api/notes", () => HttpResponse.json(refused, { status: 500 })),
  );

  renderWithQuery(<NewNoteForm />);
  await submitNote("Title");

  expect(await screen.findByText("Couldn't save")).toBeInTheDocument();
  expect(screen.getByText("internal error")).toBeInTheDocument();
  expect(screen.getByLabelText("Title")).toHaveValue("Title");
});

function note(n: number): NoteResponse {
  return {
    id: String(n),
    authorId: "a1",
    title: `Note ${n}`,
    body: "",
    createdAt: "2026-01-01T00:00:00Z",
    version: "1",
  };
}

test("pages through notes on the server", async () => {
  const notes = Array.from({ length: 25 }, (_, i) => note(i + 1));
  server.use(
    http.get("*/api/notes", ({ request }) => {
      const url = new URL(request.url);
      const limit = Number(url.searchParams.get("limit"));
      const offset = Number(url.searchParams.get("offset"));
      return HttpResponse.json({
        items: notes.slice(offset, offset + limit),
        total: notes.length,
        limit,
        offset,
      });
    }),
  );

  const user = userEvent.setup();
  renderWithQuery(<NotesTable />);

  expect(await screen.findByText("Note 20")).toBeInTheDocument();
  expect(screen.queryByText("Note 21")).not.toBeInTheDocument();
  expect(screen.getByText("1–20 of 25")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Previous page" })).toBeDisabled();

  await user.click(screen.getByRole("button", { name: "Next page" }));

  expect(await screen.findByText("Note 21")).toBeInTheDocument();
  expect(screen.queryByText("Note 20")).not.toBeInTheDocument();
  expect(screen.getByText("21–25 of 25")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
});

test("says so when there are no notes", async () => {
  server.use(http.get("*/api/notes", () => HttpResponse.json(page([]))));

  renderWithQuery(<NotesTable />);

  expect(await screen.findByText("No notes yet.")).toBeInTheDocument();
  expect(screen.getByText("0–0 of 0")).toBeInTheDocument();
});

test("hides a column the user switches off", async () => {
  server.use(http.get("*/api/notes", () => HttpResponse.json(page([note(1)]))));
  const headers = () => screen.queryAllByRole("columnheader").map((th) => th.textContent);

  const user = userEvent.setup();
  renderWithQuery(<NotesTable />);

  await screen.findByText("Note 1");
  expect(headers()).toEqual(["Title", "Author", "Created"]);

  await user.click(screen.getByRole("button", { name: "Columns" }));
  await user.click(await screen.findByRole("menuitemcheckbox", { name: "Author" }));

  await waitFor(() => expect(headers()).toEqual(["Title", "Created"]));
});

test("sorts notes on the server when a column header is clicked", async () => {
  const notes = [note(1), note(2), note(3)].map((n, i) => ({ ...n, title: ["B", "C", "A"][i] }));
  const requested: string[][] = [];
  server.use(
    http.get("*/api/notes", ({ request }) => {
      const sort = new URL(request.url).searchParams.getAll("sort");
      requested.push(sort);
      const sorted = [...notes];
      if (sort[0] === "title") sorted.sort((a, b) => a.title.localeCompare(b.title));
      if (sort[0] === "-title") sorted.sort((a, b) => b.title.localeCompare(a.title));
      return HttpResponse.json(page(sorted));
    }),
  );
  const titles = () =>
    screen
      .getAllByRole("row")
      .slice(1)
      .map((row) => (row as HTMLTableRowElement).cells[0].textContent);

  const user = userEvent.setup();
  renderWithQuery(<NotesTable />);

  await screen.findByText("B");
  expect(titles()).toEqual(["B", "C", "A"]);
  expect(screen.queryByRole("button", { name: "Author" })).not.toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "Title" }));
  await waitFor(() => expect(titles()).toEqual(["A", "B", "C"]));
  expect(screen.getByRole("columnheader", { name: "Title" })).toHaveAttribute(
    "aria-sort",
    "ascending",
  );

  await user.click(screen.getByRole("button", { name: "Title" }));
  await waitFor(() => expect(titles()).toEqual(["C", "B", "A"]));

  expect(requested).toEqual([[], ["title"], ["-title"]]);
});

test("shows a list that isn't paged without a pager", async () => {
  server.use(http.get("*/api/notes", () => HttpResponse.json({ items: [note(1), note(2)] })));

  renderWithQuery(<NotesTable />);

  expect(await screen.findByText("Note 2")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Next page" })).not.toBeInTheDocument();
});
