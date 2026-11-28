import { auditApi, auditEntry, notAnAdmin } from "@/test/access-fixtures";
import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

async function auditTable() {
  const region = await screen.findByRole("region", { name: "Audit log" });
  return within(region).findByRole("table");
}

test("lists the audit log newest first, the system's changes as System", async () => {
  server.use(
    ...auditApi([
      auditEntry(1, { change: "roleCreated", actor: undefined, subject: undefined }),
      auditEntry(2, { subject: "auth0|bob" }),
    ]).handlers,
  );
  renderAt("/access/audit");

  const table = await auditTable();
  const rows = await within(table).findAllByRole("row");
  expect(within(rows[1]).getByText("Role assigned")).toBeInTheDocument();
  expect(within(rows[1]).getByText("To auth0|bob")).toBeInTheDocument();
  expect(within(rows[2]).getByText("Role created")).toBeInTheDocument();
  expect(within(rows[2]).getByText("System")).toBeInTheDocument();
});

test("filters by change and actor, kept in the URL", async () => {
  const user = userEvent.setup();
  const api = auditApi([
    auditEntry(1, { change: "roleCreated", actor: "alice" }),
    auditEntry(2, { actor: "bob" }),
  ]);
  server.use(...api.handlers);
  const { router } = renderAt("/access/audit");
  await auditTable();

  await user.selectOptions(screen.getByLabelText("Filter by change"), "roleCreated");
  await waitFor(() =>
    expect(router.state.location.search).toMatchObject({ change: "roleCreated" }),
  );
  await user.type(screen.getByLabelText("Filter by who made it"), "alice{Enter}");
  await waitFor(() =>
    expect(router.state.location.search).toMatchObject({ change: "roleCreated", actor: "alice" }),
  );
  await waitFor(() => expect(api.requests.at(-1)?.get("actor")).toBe("alice"));
  expect(api.requests.at(-1)?.get("change")).toBe("roleCreated");
  const table = await auditTable();
  await waitFor(() => expect(within(table).getAllByRole("row")).toHaveLength(2));
});

test("sends a date range as the start of each day, until inclusive", async () => {
  const api = auditApi([auditEntry(5)]);
  server.use(...api.handlers);
  renderAt("/access/audit?since=2026-01-03&until=2026-01-05");

  await auditTable();
  const query = api.requests.at(-1);
  expect(query?.get("since")).toBe(new Date("2026-01-03T00:00").toISOString());
  expect(query?.get("until")).toBe(new Date("2026-01-06T00:00").toISOString());
});

test("says the log is still being built on a 503", async () => {
  server.use(
    http.get("*/api/access/audit", () =>
      HttpResponse.json(
        { code: "unavailable", message: "still being built", requestId: "test" },
        { status: 503 },
      ),
    ),
    ...auditApi([]).handlers,
  );
  renderAt("/access/audit");

  expect(await screen.findByText(/still being built/)).toBeInTheDocument();
});

test("hides the Audit log area from a caller who may not list it, and shows the refusal when linked", async () => {
  server.use(...notAnAdmin);
  renderAt("/access/audit");

  expect(await screen.findByText(/You may not manage this/)).toBeInTheDocument();
  expect(screen.queryByRole("link", { name: "Audit log" })).not.toBeInTheDocument();
});
