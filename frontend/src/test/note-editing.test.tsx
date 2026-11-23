import { note, notesApi, refusal } from "@/test/fixtures";
import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

async function openEditor(title: string) {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: `Edit ${title}` }));
  return { user, dialog: await screen.findByRole("dialog", { name: "Edit note" }) };
}

test("edits a note's title and body, sending the version it read", async () => {
  const api = notesApi([note(1, { body: "Milk", version: "3" })]);
  server.use(...api.handlers);
  renderAt("/notes");

  const { user, dialog } = await openEditor("Note 1");
  const title = within(dialog).getByLabelText("Title");
  const body = within(dialog).getByLabelText("Body");
  expect(title).toHaveValue("Note 1");
  expect(body).toHaveValue("Milk");
  await user.clear(title);
  await user.type(title, "Shopping");
  await user.type(body, ", eggs");
  await user.click(within(dialog).getByRole("button", { name: "Save" }));

  await waitFor(() =>
    expect(screen.queryByRole("dialog", { name: "Edit note" })).not.toBeInTheDocument(),
  );
  expect(await screen.findByText("Shopping")).toBeInTheDocument();
  expect(screen.getByText("Note saved")).toBeInTheDocument();
  expect(api.ifMatches).toEqual(['"3"']);
});

test("sends back the note's archiveAt, which it doesn't show, so its archive stands", async () => {
  const archiveAt = "2026-03-01T00:00:00Z";
  const edits: unknown[] = [];
  const api = notesApi([note(1, { archiveAt })]);
  server.use(
    http.patch("*/api/notes/:id", async ({ request }) => {
      edits.push(await request.clone().json());
    }),
    ...api.handlers,
  );
  renderAt("/notes");

  const { user, dialog } = await openEditor("Note 1");
  await user.type(within(dialog).getByLabelText("Title"), ", renamed");
  await user.click(within(dialog).getByRole("button", { name: "Save" }));

  expect(await screen.findByText("Note 1, renamed")).toBeInTheDocument();
  expect(edits).toEqual([{ title: "Note 1, renamed", body: "", archiveAt }]);
});

test("when someone else changed the note meanwhile, it reloads, says so and keeps the edit", async () => {
  const items = [note(1)];
  const api = notesApi(items);
  server.use(...api.handlers);
  renderAt("/notes");
  const { user, dialog } = await openEditor("Note 1");
  items[0] = { ...items[0], title: "Renamed elsewhere", version: "2" };

  const title = within(dialog).getByLabelText("Title");
  await user.clear(title);
  await user.type(title, "Mine");
  await user.click(within(dialog).getByRole("button", { name: "Save" }));

  expect(await within(dialog).findByText(/Someone else changed this note/)).toBeInTheDocument();
  expect(title).toHaveValue("Mine");
  expect(await screen.findByText("Renamed elsewhere")).toBeInTheDocument();
  expect(screen.queryByText("Couldn't save")).not.toBeInTheDocument();

  await user.click(within(dialog).getByRole("button", { name: "Save" }));

  await waitFor(() =>
    expect(screen.queryByRole("dialog", { name: "Edit note" })).not.toBeInTheDocument(),
  );
  expect(await screen.findByText("Mine")).toBeInTheDocument();
  expect(api.ifMatches).toEqual(['"1"', '"2"']);
});

test("shows a refused title beside the Title field and keeps the dialog open", async () => {
  const message = "title must be at most 200 characters";
  server.use(
    http.patch("*/api/notes/:id", () =>
      HttpResponse.json(refusal("validation_error", message, "title"), { status: 400 }),
    ),
    ...notesApi([note(1)]).handlers,
  );
  renderAt("/notes");

  const { user, dialog } = await openEditor("Note 1");
  await user.type(within(dialog).getByLabelText("Title"), " that is far too long");
  await user.click(within(dialog).getByRole("button", { name: "Save" }));

  const title = within(dialog).getByLabelText("Title");
  expect(await within(dialog).findByText(message)).toBeInTheDocument();
  expect(title).toHaveAttribute("aria-invalid", "true");
  expect(title).toHaveAccessibleDescription(message);
  expect(screen.getAllByText(message)).toHaveLength(1);
});

test("an archived note can't be edited", async () => {
  server.use(...notesApi([note(1), note(2, { archivedAt: "2026-02-01T00:00:00Z" })]).handlers);
  renderAt("/notes");

  expect(await screen.findByRole("button", { name: "Edit Note 1" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Edit Note 2" })).not.toBeInTheDocument();
});
