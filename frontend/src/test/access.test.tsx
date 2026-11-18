import { accessApi, grant, notAnAdmin, role } from "@/test/access-fixtures";
import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

function grantsTable() {
  return screen.findByRole("region", { name: "Grants" });
}

/** The grant dialog's role picker, once it offers the roles. */
async function roleField(dialog: HTMLElement) {
  await within(dialog).findByRole("option", { name: "support" });
  return within(dialog).getByLabelText("Role");
}

async function rolesTab() {
  const user = userEvent.setup();
  renderAt("/access?tab=roles");
  const table = await screen.findByRole("region", { name: "Roles" });
  await within(table).findByText("support");
  return { user, table };
}

test("hides the access screen and its link from a caller who isn't an admin", async () => {
  server.use(...notAnAdmin);
  renderAt("/access");

  expect(await screen.findByText(/Only an admin can manage/)).toBeInTheDocument();
  expect(screen.queryByRole("link", { name: "Access" })).not.toBeInTheDocument();
  expect(screen.queryByRole("region", { name: "Grants" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Grant access" })).not.toBeInTheDocument();
});

test("lists the grants for an admin, marking a stale one", async () => {
  server.use(
    ...accessApi([
      grant(1),
      grant(2, { role: "support", action: "polish", stale: true, grantedBy: "an-admin" }),
    ]).handlers,
  );
  renderAt("/access");

  const table = await grantsTable();
  expect(await within(table).findByText("polish")).toBeInTheDocument();
  expect(within(table).getByText("Stale")).toBeInTheDocument();
  expect(within(table).getByText("Default")).toBeInTheDocument();
  expect(within(table).getByText("an-admin")).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Access" })).toBeInTheDocument();
});

test("filters the grants by role, keeping the filter in the URL", async () => {
  const api = accessApi([grant(1), grant(2, { role: "support" })]);
  server.use(...api.handlers);
  const { router } = renderAt("/access");
  const user = userEvent.setup();
  const table = await grantsTable();
  await within(table).findByText("support");

  await user.type(screen.getByLabelText("Filter by role"), "support{Enter}");

  await waitFor(() => expect(within(table).queryByText("user")).not.toBeInTheDocument());
  expect(router.state.location.search).toMatchObject({ role: "support" });
  expect(api.requests.at(-1)?.get("role")).toBe("support");
});

test("grants a role an action chosen from the catalog", async () => {
  const api = accessApi([grant(1)]);
  server.use(...api.handlers);
  renderAt("/access");
  const user = userEvent.setup();
  await grantsTable();

  await user.click(screen.getByRole("button", { name: "Grant access" }));
  const dialog = await screen.findByRole("dialog", { name: "Grant access" });
  await waitFor(() =>
    expect(within(dialog).getByRole("option", { name: "gadget (workshop)" })).toBeInTheDocument(),
  );
  await user.selectOptions(await roleField(dialog), "support");
  await user.selectOptions(within(dialog).getByLabelText("Resource"), "gadget");
  await user.selectOptions(within(dialog).getByLabelText("Action"), "view");
  await user.selectOptions(within(dialog).getByLabelText("Scope"), "maker");
  expect(within(dialog).getByRole("option", { name: "scrap (granted to no one)" })).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Grant" }));

  await waitFor(() =>
    expect(screen.queryByRole("dialog", { name: "Grant access" })).not.toBeInTheDocument(),
  );
  const table = await grantsTable();
  expect(await within(table).findByText("maker")).toBeInTheDocument();
  expect(screen.getByText("Access granted")).toBeInTheDocument();
});

test("shows a refused grant's error beside the field at fault", async () => {
  const message = "`fly` is not an action of `gadget`";
  server.use(
    http.post("*/api/access/grants", () =>
      HttpResponse.json(
        {
          code: "validation_error",
          message,
          fields: [{ path: "action", message }],
          requestId: "test",
        },
        { status: 400 },
      ),
    ),
    ...accessApi([]).handlers,
  );
  renderAt("/access");
  const user = userEvent.setup();
  await grantsTable();

  await user.click(screen.getByRole("button", { name: "Grant access" }));
  const dialog = await screen.findByRole("dialog", { name: "Grant access" });
  await waitFor(() =>
    expect(within(dialog).getByRole("option", { name: "gadget (workshop)" })).toBeInTheDocument(),
  );
  await user.selectOptions(await roleField(dialog), "support");
  await user.selectOptions(within(dialog).getByLabelText("Resource"), "gadget");
  await user.selectOptions(within(dialog).getByLabelText("Action"), "make");
  await user.click(within(dialog).getByRole("button", { name: "Grant" }));

  expect(await within(dialog).findByText(message)).toBeInTheDocument();
  expect(within(dialog).getByLabelText("Action")).toHaveAttribute("aria-invalid", "true");
});

test("revokes a grant once confirmed, sending the version listed", async () => {
  const api = accessApi([grant(1, { version: "3" }), grant(2, { role: "support" })]);
  server.use(...api.handlers);
  renderAt("/access");
  const user = userEvent.setup();
  const table = await grantsTable();

  await user.click(
    await within(table).findByRole("button", { name: "Revoke support → gadget:make (any)" }),
  );
  const confirm = await screen.findByRole("alertdialog", { name: "Revoke this grant?" });
  await user.click(within(confirm).getByRole("button", { name: "Revoke" }));

  await waitFor(() => expect(within(table).queryByText("support")).not.toBeInTheDocument());
  expect(api.ifMatches).toEqual(['"1"']);
  expect(screen.getByText("Grant revoked")).toBeInTheDocument();
});

test("a grant changed meanwhile isn't revoked: it says so and reloads the list", async () => {
  const items = [grant(1, { role: "support" })];
  const api = accessApi(items);
  server.use(...api.handlers);
  renderAt("/access");
  const user = userEvent.setup();
  const table = await grantsTable();
  await user.click(
    await within(table).findByRole("button", { name: "Revoke support → gadget:make (any)" }),
  );
  items[0] = { ...items[0], scope: "maker", version: "2" };

  const confirm = await screen.findByRole("alertdialog", { name: "Revoke this grant?" });
  await user.click(within(confirm).getByRole("button", { name: "Revoke" }));

  expect(await screen.findByText(/Someone else changed this grant/)).toBeInTheDocument();
  expect(await within(table).findByText("maker")).toBeInTheDocument();
  expect(screen.queryByText("Grant revoked")).not.toBeInTheDocument();
  expect(api.ifMatches).toEqual(['"1"']);
});

test("the grant dialog offers only the roles that exist", async () => {
  server.use(...accessApi([]).handlers);
  renderAt("/access");
  const user = userEvent.setup();
  await grantsTable();

  await user.click(screen.getByRole("button", { name: "Grant access" }));
  const dialog = await screen.findByRole("dialog", { name: "Grant access" });
  const roles = within(await roleField(dialog)).getAllByRole("option");

  expect(roles.map((option) => option.textContent)).toEqual([
    "Choose a role",
    "admin",
    "support",
    "user",
  ]);
});

test("lists the roles on their tab, kept in the URL, marking the built-in ones", async () => {
  server.use(...accessApi([]).handlers);
  const { router } = renderAt("/access");
  const user = userEvent.setup();
  await grantsTable();

  await user.click(screen.getByRole("tab", { name: "Roles" }));

  const table = await screen.findByRole("region", { name: "Roles" });
  expect(await within(table).findByText("Answers customers")).toBeInTheDocument();
  expect(within(table).getAllByText("Built in")).toHaveLength(2);
  expect(within(table).getByRole("button", { name: "Delete support" })).toBeInTheDocument();
  expect(within(table).queryByRole("button", { name: "Delete admin" })).not.toBeInTheDocument();
  expect(router.state.location.search).toMatchObject({ tab: "roles" });
});

test("creates a role", async () => {
  server.use(...accessApi([]).handlers);
  const { user, table } = await rolesTab();

  await user.click(screen.getByRole("button", { name: "Create role" }));
  const dialog = await screen.findByRole("dialog", { name: "Create role" });
  await user.type(within(dialog).getByLabelText("Name"), "auditor");
  await user.type(within(dialog).getByLabelText("Description"), "Reads the grants");
  await user.click(within(dialog).getByRole("button", { name: "Create" }));

  await waitFor(() =>
    expect(screen.queryByRole("dialog", { name: "Create role" })).not.toBeInTheDocument(),
  );
  expect(await within(table).findByText("auditor")).toBeInTheDocument();
  expect(within(table).getByText("Reads the grants")).toBeInTheDocument();
  expect(screen.getByText("Role created")).toBeInTheDocument();
});

test("a role whose name exists isn't created: it says so beside the name", async () => {
  server.use(...accessApi([]).handlers);
  const { user } = await rolesTab();

  await user.click(screen.getByRole("button", { name: "Create role" }));
  const dialog = await screen.findByRole("dialog", { name: "Create role" });
  await user.type(within(dialog).getByLabelText("Name"), "support");
  await user.click(within(dialog).getByRole("button", { name: "Create" }));

  expect(await within(dialog).findByText("a role of that name exists already")).toBeInTheDocument();
  expect(within(dialog).getByLabelText("Name")).toHaveAttribute("aria-invalid", "true");
  expect(screen.queryByText("Couldn't save")).not.toBeInTheDocument();
});

test("deletes a role once confirmed, sending the version listed", async () => {
  const api = accessApi([], [role("admin"), role("support", { version: "3" }), role("user")]);
  server.use(...api.handlers);
  const { user, table } = await rolesTab();

  await user.click(within(table).getByRole("button", { name: "Delete support" }));
  const confirm = await screen.findByRole("alertdialog", { name: "Delete this role?" });
  await user.click(within(confirm).getByRole("button", { name: "Delete" }));

  await waitFor(() => expect(within(table).queryByText("support")).not.toBeInTheDocument());
  expect(api.ifMatches).toEqual(['"3"']);
  expect(screen.getByText("Role deleted")).toBeInTheDocument();
});

test("a role a grant still names isn't deleted: it says why", async () => {
  server.use(...accessApi([grant(1, { role: "support" })]).handlers);
  const { user, table } = await rolesTab();

  await user.click(within(table).getByRole("button", { name: "Delete support" }));
  const confirm = await screen.findByRole("alertdialog", { name: "Delete this role?" });
  await user.click(within(confirm).getByRole("button", { name: "Delete" }));

  expect(await screen.findByText(/a grant still names that role/)).toBeInTheDocument();
  expect(within(table).getByText("support")).toBeInTheDocument();
  expect(screen.queryByText("Role deleted")).not.toBeInTheDocument();
});
