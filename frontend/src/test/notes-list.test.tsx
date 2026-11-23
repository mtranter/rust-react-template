import { listRequests, notes, notesApi } from "@/test/fixtures";
import { renderAt } from "@/test/render-app";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

function listOf(count: number) {
  const list = listRequests("*/api/notes");
  server.use(list.handler, ...notesApi(notes(count)).handlers);
  return list;
}

test("a link to a page and sort requests those rows and shows the sort on its column", async () => {
  const list = listOf(25);

  const { router } = renderAt("/notes?page=2&sort=-createdAt");

  expect(await screen.findByText("Note 5")).toBeInTheDocument();
  expect(screen.queryByText("Note 25")).not.toBeInTheDocument();
  expect(list.requests).toContainEqual({ offset: "20", sort: "-createdAt" });
  expect(screen.getByRole("columnheader", { name: "Created" })).toHaveAttribute(
    "aria-sort",
    "descending",
  );
  expect(router.state.location.href).toBe("/notes?page=2&sort=-createdAt");
});

test("paging moves the page in the URL, and Back returns to the rows before", async () => {
  listOf(25);
  const user = userEvent.setup();
  const { router } = renderAt("/notes");
  await screen.findByText("Note 20");

  await user.click(screen.getByRole("button", { name: "Next page" }));

  expect(await screen.findByText("Note 21")).toBeInTheDocument();
  expect(router.state.location.href).toBe("/notes?page=2");

  router.history.back();

  expect(await screen.findByText("Note 20")).toBeInTheDocument();
  await waitFor(() => expect(router.state.location.href).toBe("/notes"));
});

test("sorting puts the sort in the URL and goes back to the first page", async () => {
  const list = listOf(25);
  const user = userEvent.setup();
  const { router } = renderAt("/notes?page=2");
  await screen.findByText("Note 21");

  await user.click(screen.getByRole("button", { name: "Title" }));

  await waitFor(() => expect(router.state.location.href).toBe("/notes?sort=title"));
  expect(list.requests.at(-1)).toEqual({ offset: "0", sort: "title" });
  expect(screen.getByRole("columnheader", { name: "Title" })).toHaveAttribute(
    "aria-sort",
    "ascending",
  );
});

test("a malformed page or an unsortable column in a link falls back to the defaults", async () => {
  const list = listOf(3);

  renderAt("/notes?page=abc&sort=authorId");

  expect(await screen.findByText("Note 1")).toBeInTheDocument();
  expect(list.requests).toEqual([{ offset: "0", sort: null }]);
});

test("a link past the last page says so and offers the first", async () => {
  listOf(3);
  const user = userEvent.setup();
  const { router } = renderAt("/notes?page=3");

  expect(await screen.findByText("Nothing on page 3.")).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "Go to page 1" }));

  expect(await screen.findByText("Note 1")).toBeInTheDocument();
  expect(router.state.location.href).toBe("/notes");
});

test("an old /home link goes on to the notes, keeping its page", async () => {
  listOf(25);

  const { router } = renderAt("/home?page=2");

  expect(await screen.findByText("Note 21")).toBeInTheDocument();
  expect(router.state.location.href).toBe("/notes?page=2");
});
