import { accessApi, grant, notAnAdmin, role, subject } from "@/test/access-fixtures";
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
  for (const tab of ["Grants", "Roles", "Users"]) {
    expect(screen.queryByRole("tab", { name: tab })).not.toBeInTheDocument();
  }
});

test("hides the Users tab from a caller who isn't an admin, even linked to it", async () => {
  server.use(...notAnAdmin);
  renderAt("/access?tab=subjects");

  expect(await screen.findByText(/Only an admin can manage/)).toBeInTheDocument();
  expect(screen.queryByRole("region", { name: "Users" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Add user" })).not.toBeInTheDocument();
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

async function usersTab(path = "/access?tab=subjects") {
  const user = userEvent.setup();
  const rendered = renderAt(path);
  const table = await screen.findByRole("region", { name: "Users" });
  await within(table).findByText("Bob Babbage");
  return { user, table, ...rendered };
}

function rolesOf(table: HTMLElement, label: string) {
  return within(table).getByRole("list", { name: `Roles of ${label}` });
}

test("lists everyone on the Users tab, kept in the URL, with their roles", async () => {
  server.use(...accessApi([]).handlers);
  const { router } = renderAt("/access");
  const user = userEvent.setup();
  await grantsTable();

  await user.click(screen.getByRole("tab", { name: "Users" }));

  const table = await screen.findByRole("region", { name: "Users" });
  expect(await within(table).findByText("Ada Lovelace")).toBeInTheDocument();
  expect(within(table).getByText("ada@example.com")).toBeInTheDocument();
  expect(within(table).getByText("auth0|ada")).toBeInTheDocument();
  expect(within(rolesOf(table, "Ada Lovelace")).getByText("support")).toBeInTheDocument();
  expect(within(table).getByText("Never signed in")).toBeInTheDocument();
  expect(router.state.location.search).toMatchObject({ tab: "subjects" });
});

test("searches the Users tab by name, email and subject, keeping the search in the URL", async () => {
  const api = accessApi([]);
  server.use(...api.handlers);
  const { user, table, router } = await usersTab();

  for (const [text, shown, hidden] of [
    ["lovelace", "Ada Lovelace", "Bob Babbage"],
    ["ENGINES.org", "Bob Babbage", "Ada Lovelace"],
    ["auth0|ada", "Ada Lovelace", "Bob Babbage"],
  ]) {
    const search = screen.getByLabelText("Search by name, email or subject");
    await user.clear(search);
    await user.type(search, `${text}{Enter}`);

    await waitFor(() => expect(within(table).queryByText(hidden)).not.toBeInTheDocument());
    expect(within(table).getByText(shown)).toBeInTheDocument();
    expect(router.state.location.search).toMatchObject({ search: text });
    expect(api.requests.at(-1)?.get("search")).toBe(text);
  }
});

test("assigns a user a role chosen from the roles that can be assigned", async () => {
  server.use(...accessApi([]).handlers);
  const { user, table } = await usersTab();

  await user.click(within(table).getByRole("button", { name: "Assign a role to Bob Babbage" }));
  const dialog = await screen.findByRole("dialog", { name: "Assign a role to Bob Babbage" });
  await within(dialog).findByRole("option", { name: "support" });
  const picker = within(dialog).getByLabelText("Role");
  expect(
    within(picker)
      .getAllByRole("option")
      .map((o) => o.textContent),
  ).toEqual(["Choose a role", "admin", "support"]);
  await user.selectOptions(picker, "support");
  await user.click(within(dialog).getByRole("button", { name: "Assign" }));

  expect(await screen.findByText("Role assigned")).toBeInTheDocument();
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(await within(rolesOf(table, "Bob Babbage")).findByText("support")).toBeInTheDocument();
});

test("a role the user holds already isn't assigned again: it says so beside the role", async () => {
  server.use(...accessApi([]).handlers);
  const { user, table } = await usersTab();

  await user.click(within(table).getByRole("button", { name: "Assign a role to Ada Lovelace" }));
  const dialog = await screen.findByRole("dialog", { name: "Assign a role to Ada Lovelace" });
  await within(dialog).findByRole("option", { name: "support" });
  await user.selectOptions(within(dialog).getByLabelText("Role"), "support");
  await user.click(within(dialog).getByRole("button", { name: "Assign" }));

  expect(
    await within(dialog).findByText("that role is assigned to that subject already"),
  ).toBeInTheDocument();
  expect(within(dialog).getByLabelText("Role")).toHaveAttribute("aria-invalid", "true");
  expect(screen.queryByText("Role assigned")).not.toBeInTheDocument();
});

test("removes a role from a user once confirmed, sending the version listed", async () => {
  const api = accessApi([]);
  server.use(...api.handlers);
  const { user, table } = await usersTab();

  await user.click(within(table).getByRole("button", { name: "Remove support from Ada Lovelace" }));
  const confirm = await screen.findByRole("alertdialog", { name: "Remove this role?" });
  await user.click(within(confirm).getByRole("button", { name: "Remove" }));

  await waitFor(() =>
    expect(within(rolesOf(table, "Ada Lovelace")).queryByText("support")).not.toBeInTheDocument(),
  );
  expect(api.ifMatches).toEqual(['"2"']);
  expect(screen.getByText("Role removed")).toBeInTheDocument();
});

test("a role changed meanwhile isn't removed: it says so and reloads the list", async () => {
  const subjects = [
    subject("auth0|bob", { name: "Bob Babbage", roles: [{ role: "support", version: "1" }] }),
  ];
  const api = accessApi([], undefined, subjects);
  server.use(...api.handlers);
  const { user, table } = await usersTab();
  await user.click(within(table).getByRole("button", { name: "Remove support from Bob Babbage" }));
  subjects[0].roles[0].version = "3";

  const confirm = await screen.findByRole("alertdialog", { name: "Remove this role?" });
  await user.click(within(confirm).getByRole("button", { name: "Remove" }));

  expect(await screen.findByText(/Someone else changed this user's roles/)).toBeInTheDocument();
  // Bob is the only user, so his is the only role listed.
  expect(within(table).getByText("support")).toBeInTheDocument();
  expect(screen.queryByText("Role removed")).not.toBeInTheDocument();
});

test("adds a user who hasn't signed in yet by their subject", async () => {
  server.use(...accessApi([]).handlers);
  const { user, table } = await usersTab();

  await user.click(screen.getByRole("button", { name: "Add user" }));
  const dialog = await screen.findByRole("dialog", { name: "Add user" });
  await within(dialog).findByRole("option", { name: "support" });
  await user.type(within(dialog).getByLabelText("Subject"), "  auth0|carol ");
  await user.selectOptions(within(dialog).getByLabelText("Role"), "support");
  await user.click(within(dialog).getByRole("button", { name: "Assign" }));

  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(await within(table).findByText("auth0|carol")).toBeInTheDocument();
  expect(within(rolesOf(table, "auth0|carol")).getByText("support")).toBeInTheDocument();
});

test("shows a bootstrap admin as such, offering no removal of their admin", async () => {
  server.use(
    ...accessApi([], undefined, [
      subject("root", {
        name: "Root",
        bootstrapAdmin: true,
        roles: [
          { role: "admin", version: "1" },
          { role: "support", version: "1" },
        ],
      }),
      subject("auth0|bob", { name: "Bob Babbage" }),
    ]).handlers,
  );
  const { table } = await usersTab();

  expect(within(table).getByText("Bootstrap admin")).toBeInTheDocument();
  const roles = rolesOf(table, "Root");
  expect(within(roles).getByText("admin")).toBeInTheDocument();
  expect(
    within(table).queryByRole("button", { name: "Remove admin from Root" }),
  ).not.toBeInTheDocument();
  expect(
    within(table).getByRole("button", { name: "Remove support from Root" }),
  ).toBeInTheDocument();
});
