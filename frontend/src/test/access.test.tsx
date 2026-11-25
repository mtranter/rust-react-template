import {
  accessApi,
  adminPermissions,
  grant,
  notAnAdmin,
  role,
  subject,
} from "@/test/access-fixtures";
import { holding } from "@/test/may-use-everything";
import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

/** The grants listed, without the filters above them, whose options name roles and actions too. */
async function grantsTable() {
  const region = await screen.findByRole("region", { name: "Permissions" });
  return within(region).findByRole("table");
}

/** The grant dialog's role picker, once it offers the roles. */
async function roleField(dialog: HTMLElement) {
  await within(dialog).findByRole("option", { name: "support" });
  return within(dialog).getByLabelText("Role");
}

async function rolesPage() {
  const user = userEvent.setup();
  renderAt("/access/roles");
  const table = await screen.findByRole("region", { name: "Roles" });
  await within(table).findByText("support");
  return { user, table };
}

test("hides the access screen and its link from a caller who holds none of its actions", async () => {
  server.use(...notAnAdmin);
  const { queryClient } = renderAt("/");

  await screen.findByRole("heading", { level: 1 });
  await waitFor(() => expect(queryClient.isFetching()).toBe(0));
  expect(screen.queryByRole("region", { name: "Platform" })).not.toBeInTheDocument();
  expect(screen.queryByRole("link", { name: /Access/ })).not.toBeInTheDocument();
});

test("shows the server's refusal on an access page linked to directly by a caller who may not", async () => {
  server.use(...notAnAdmin);
  renderAt("/access/subjects");

  expect(await screen.findByText(/You may not manage this/)).toBeInTheDocument();
  expect(screen.queryByRole("navigation", { name: "Access" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Add user" })).not.toBeInTheDocument();
  expect(screen.queryByRole("table")).not.toBeInTheDocument();
});

test("shows Access with Roles and Permissions but no Users to a caller who may not list subjects", async () => {
  server.use(
    holding(adminPermissions.filter((p) => p.resource !== "subject")),
    ...accessApi([]).handlers,
  );
  renderAt("/access/roles");

  const areas = await screen.findByRole("navigation", { name: "Access" });
  expect(within(areas).getByRole("link", { name: "Roles" })).toBeInTheDocument();
  expect(within(areas).getByRole("link", { name: "Permissions" })).toBeInTheDocument();
  expect(within(areas).queryByRole("link", { name: "Users" })).not.toBeInTheDocument();
});

test("opens Access at the first area the caller may use", async () => {
  server.use(
    holding(adminPermissions.filter((p) => p.resource !== "subject")),
    ...accessApi([]).handlers,
  );
  const user = userEvent.setup();
  const { router } = renderAt("/");

  const platform = await screen.findByRole("region", { name: "Platform" });
  await user.click(within(platform).getByRole("link", { name: /Access/ }));

  expect(await screen.findByRole("heading", { name: "Roles" })).toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/access/roles");
});

test("lists the grants for an admin, marking a stale one", async () => {
  server.use(
    ...accessApi([
      grant(1),
      grant(2, { role: "support", action: "polish", stale: true, grantedBy: "an-admin" }),
    ]).handlers,
  );
  renderAt("/access/permissions");

  const table = await grantsTable();
  expect(await within(table).findByText("polish")).toBeInTheDocument();
  expect(within(table).getByText("No longer used")).toBeInTheDocument();
  expect(within(table).getByText("Built in")).toBeInTheDocument();
  expect(within(table).getByText("an-admin")).toBeInTheDocument();
  const platform = screen.getByRole("navigation", { name: "Platform" });
  expect(within(platform).getByRole("link", { name: "Access" })).toBeInTheDocument();
});

test("filters the grants by role, keeping the filter in the URL", async () => {
  const api = accessApi([grant(1), grant(2, { role: "support" })]);
  server.use(...api.handlers);
  const { router } = renderAt("/access/permissions");
  const user = userEvent.setup();
  const table = await grantsTable();
  await within(table).findByText("support");

  const roleFilter = screen.getByLabelText("Filter by role");
  await within(roleFilter).findByRole("option", { name: "support" });
  await user.selectOptions(roleFilter, "support");

  await waitFor(() => expect(within(table).queryByText("user")).not.toBeInTheDocument());
  expect(router.state.location.search).toMatchObject({ role: "support" });
  expect(api.requests.at(-1)?.get("role")).toBe("support");
});

test("offers a resource's actions to filter by, dropping an action it doesn't have", async () => {
  const api = accessApi([grant(1)]);
  server.use(...api.handlers);
  const { router } = renderAt("/access/permissions?action=use");
  const user = userEvent.setup();
  await grantsTable();
  const resourceFilter = screen.getByLabelText("Filter by resource");
  await within(resourceFilter).findByRole("option", { name: "Gadgets" });

  await user.selectOptions(resourceFilter, "gadget");

  await waitFor(() => expect(router.state.location.search).toMatchObject({ resource: "gadget" }));
  expect(router.state.location.search).not.toHaveProperty("action");
  const actions = within(screen.getByLabelText("Filter by action")).getAllByRole("option");
  expect(actions.map((o) => o.textContent)).toEqual(["Any action", "Make", "View", "Scrap"]);
});

/** Opens the grant dialog, once the catalog and the roles are in, choosing `role`. */
async function grantDialog(user: ReturnType<typeof userEvent.setup>, role = "support") {
  await grantsTable();
  await user.click(screen.getByRole("button", { name: "Grant access" }));
  const dialog = await screen.findByRole("dialog", { name: "Grant access" });
  await user.selectOptions(await roleField(dialog), role);
  return dialog;
}

/** Opens the multi-select labelled `label`, checks or unchecks each of `options`, and closes it. */
async function choose(
  user: ReturnType<typeof userEvent.setup>,
  dialog: HTMLElement,
  label: string,
  options: string[],
) {
  await user.click(within(dialog).getByLabelText(label));
  const popup = await screen.findByRole("group", { name: /^Choose/ });
  for (const option of options) {
    await user.click(within(popup).getByRole("checkbox", { name: option }));
  }
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("group", { name: /^Choose/ })).toBeNull());
}

test("grants a role several actions on several resources in several scopes at once", async () => {
  const api = accessApi([grant(1)]);
  server.use(...api.handlers);
  renderAt("/access/permissions");
  const user = userEvent.setup();
  const dialog = await grantDialog(user);

  await waitFor(() => expect(within(dialog).getByLabelText("On")).toBeEnabled());
  await choose(user, dialog, "On", ["Gadgets"]);
  await choose(user, dialog, "Can", ["Make", "View"]);
  await choose(user, dialog, "Which", ["Gadgets they made (Gadgets)"]);
  expect(within(dialog).getByLabelText("Can")).toHaveTextContent("2 selected");
  expect(within(dialog).getByLabelText("Which")).toHaveTextContent("2 selected");
  await user.click(within(dialog).getByRole("button", { name: "Grant" }));

  await waitFor(() =>
    expect(screen.queryByRole("dialog", { name: "Grant access" })).not.toBeInTheDocument(),
  );
  expect(api.bulkGrants).toEqual([
    { role: "support", resources: ["gadget"], actions: ["make", "view"], scopes: ["any", "maker"] },
  ]);
  expect(await screen.findByText("Granted 4")).toBeInTheDocument();
  const table = await grantsTable();
  expect(await within(table).findAllByText("Gadgets they made")).toHaveLength(2);
});

test("offers the actions and scopes of every resource chosen, once each, and drops the rest", async () => {
  server.use(...accessApi([]).handlers);
  renderAt("/access/permissions");
  const user = userEvent.setup();
  const dialog = await grantDialog(user);

  expect(within(dialog).getByLabelText("Can")).toBeDisabled();
  await choose(user, dialog, "On", ["Widgets", "Gadgets"]);
  await user.click(within(dialog).getByLabelText("Can"));
  const actions = await screen.findByRole("group", { name: "Choose actions" });
  expect(
    within(actions)
      .getAllByRole("checkbox")
      .map((c) => c.closest("label")?.textContent),
  ).toEqual(["Select all", "Build", "Make", "View", "Scrap (no one can yet)"]);
  const all = within(actions).getByRole("checkbox", { name: "Select all" });
  await user.click(within(actions).getByRole("checkbox", { name: "Build" }));
  expect(all).toHaveAttribute("aria-checked", "mixed");
  await user.click(all);
  expect(all).toHaveAttribute("aria-checked", "true");
  await user.keyboard("{Escape}");
  expect(within(dialog).getByLabelText("Can")).toHaveTextContent("4 selected");

  await choose(user, dialog, "On", ["Gadgets"]);
  expect(within(dialog).getByLabelText("Can")).toHaveTextContent("Build");
  await user.click(within(dialog).getByLabelText("Which"));
  const scopes = await screen.findByRole("group", { name: "Choose which" });
  expect(within(scopes).getAllByRole("checkbox")).toHaveLength(2);
  expect(within(scopes).getByRole("checkbox", { name: "Any" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
});

test("says how many of a bulk grant were made already", async () => {
  server.use(...accessApi([grant(1, { role: "admin" })]).handlers);
  renderAt("/access/permissions");
  const user = userEvent.setup();
  const dialog = await grantDialog(user, "admin");

  await choose(user, dialog, "On", ["Select all"]);
  await choose(user, dialog, "Can", ["Make", "Build"]);
  await user.click(within(dialog).getByRole("button", { name: "Grant" }));

  expect(await screen.findByText("Granted 1")).toBeInTheDocument();
  expect(screen.getByText("1 already granted")).toBeInTheDocument();
});

test("shows a refused bulk grant's error beside the control at fault", async () => {
  const message = "`fly` is not an action of `gadget`";
  server.use(
    http.post("*/api/access/grants/bulk", () =>
      HttpResponse.json(
        {
          code: "validation_error",
          message,
          fields: [{ path: "actions", message }],
          requestId: "test",
        },
        { status: 400 },
      ),
    ),
    ...accessApi([]).handlers,
  );
  renderAt("/access/permissions");
  const user = userEvent.setup();
  const dialog = await grantDialog(user);

  await choose(user, dialog, "On", ["Gadgets"]);
  await choose(user, dialog, "Can", ["Make"]);
  await user.click(within(dialog).getByRole("button", { name: "Grant" }));

  expect(await within(dialog).findByText(message)).toBeInTheDocument();
  expect(within(dialog).getByLabelText("Can")).toHaveAttribute("aria-invalid", "true");
});

test("revokes a grant once confirmed, sending the version listed", async () => {
  const api = accessApi([grant(1, { version: "3" }), grant(2, { role: "support" })]);
  server.use(...api.handlers);
  renderAt("/access/permissions");
  const user = userEvent.setup();
  const table = await grantsTable();

  await user.click(
    await within(table).findByRole("button", {
      name: "Revoke: support can Make Gadgets (Every gadget)",
    }),
  );
  const confirm = await screen.findByRole("alertdialog", { name: "Revoke this permission?" });
  await user.click(within(confirm).getByRole("button", { name: "Revoke" }));

  await waitFor(() => expect(within(table).queryByText("support")).not.toBeInTheDocument());
  expect(api.ifMatches).toEqual(['"1"']);
  expect(screen.getByText("Permission revoked")).toBeInTheDocument();
});

test("a grant changed meanwhile isn't revoked: it says so and reloads the list", async () => {
  const items = [grant(1, { role: "support" })];
  const api = accessApi(items);
  server.use(...api.handlers);
  renderAt("/access/permissions");
  const user = userEvent.setup();
  const table = await grantsTable();
  await user.click(
    await within(table).findByRole("button", {
      name: "Revoke: support can Make Gadgets (Every gadget)",
    }),
  );
  items[0] = { ...items[0], scope: "maker", version: "2" };

  const confirm = await screen.findByRole("alertdialog", { name: "Revoke this permission?" });
  await user.click(within(confirm).getByRole("button", { name: "Revoke" }));

  expect(await screen.findByText(/Someone else changed this permission/)).toBeInTheDocument();
  expect(await within(table).findByText("Gadgets they made")).toBeInTheDocument();
  expect(screen.queryByText("Permission revoked")).not.toBeInTheDocument();
  expect(api.ifMatches).toEqual(['"1"']);
});

test("the grant dialog offers only the roles that exist", async () => {
  server.use(...accessApi([]).handlers);
  renderAt("/access/permissions");
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

test("lists the roles on their page, reached from the sidebar, marking the built-in ones", async () => {
  server.use(...accessApi([]).handlers);
  const { router } = renderAt("/access");
  const user = userEvent.setup();
  await screen.findByRole("region", { name: "Users" });

  const areas = await screen.findByRole("navigation", { name: "Access" });
  await user.click(within(areas).getByRole("link", { name: "Roles" }));

  const table = await screen.findByRole("region", { name: "Roles" });
  expect(await within(table).findByText("Answers customers")).toBeInTheDocument();
  expect(within(table).getAllByText("Built in")).toHaveLength(2);
  expect(within(table).getByRole("button", { name: "Delete support" })).toBeInTheDocument();
  expect(within(table).queryByRole("button", { name: "Delete admin" })).not.toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/access/roles");
});

test("creates a role", async () => {
  server.use(...accessApi([]).handlers);
  const { user, table } = await rolesPage();

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
  const { user } = await rolesPage();

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
  const { user, table } = await rolesPage();

  await user.click(within(table).getByRole("button", { name: "Delete support" }));
  const confirm = await screen.findByRole("alertdialog", { name: "Delete this role?" });
  await user.click(within(confirm).getByRole("button", { name: "Delete" }));

  await waitFor(() => expect(within(table).queryByText("support")).not.toBeInTheDocument());
  expect(api.ifMatches).toEqual(['"3"']);
  expect(screen.getByText("Role deleted")).toBeInTheDocument();
});

test("a role a grant still names isn't deleted: it says why", async () => {
  server.use(...accessApi([grant(1, { role: "support" })]).handlers);
  const { user, table } = await rolesPage();

  await user.click(within(table).getByRole("button", { name: "Delete support" }));
  const confirm = await screen.findByRole("alertdialog", { name: "Delete this role?" });
  await user.click(within(confirm).getByRole("button", { name: "Delete" }));

  expect(await screen.findByText(/a grant still names that role/)).toBeInTheDocument();
  expect(within(table).getByText("support")).toBeInTheDocument();
  expect(screen.queryByText("Role deleted")).not.toBeInTheDocument();
});

async function usersPage(path = "/access/subjects") {
  const user = userEvent.setup();
  const rendered = renderAt(path);
  const table = await screen.findByRole("region", { name: "Users" });
  await within(table).findByText("Bob Babbage");
  return { user, table, ...rendered };
}

function rolesOf(table: HTMLElement, label: string) {
  return within(table).getByRole("list", { name: `Roles of ${label}` });
}

test("opens on the Users page, listing everyone with their roles, their ids kept out of sight", async () => {
  server.use(...accessApi([]).handlers);
  renderAt("/access");

  const table = await screen.findByRole("region", { name: "Users" });
  expect(await within(table).findByText("Ada Lovelace")).toBeInTheDocument();
  expect(within(table).getByText("ada@example.com")).toBeInTheDocument();
  expect(within(table).queryByText("auth0|ada")).not.toBeInTheDocument();
  expect(within(table).getByTitle("auth0|ada")).toHaveTextContent("Ada Lovelace");
  expect(within(rolesOf(table, "Ada Lovelace")).getByText("support")).toBeInTheDocument();
  expect(within(table).getByText("Never signed in")).toBeInTheDocument();
});

test("returns to the Users page from another, through the sidebar", async () => {
  server.use(...accessApi([]).handlers);
  const { router } = renderAt("/access/roles");
  const user = userEvent.setup();
  await screen.findByRole("region", { name: "Roles" });

  const areas = await screen.findByRole("navigation", { name: "Access" });
  await user.click(within(areas).getByRole("link", { name: "Users" }));

  expect(await screen.findByRole("region", { name: "Users" })).toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/access/subjects");
});

test("searches the Users page by name, email and subject, keeping the search in the URL", async () => {
  const api = accessApi([]);
  server.use(...api.handlers);
  const { user, table, router } = await usersPage();

  for (const [text, shown, hidden] of [
    ["lovelace", "Ada Lovelace", "Bob Babbage"],
    ["ENGINES.org", "Bob Babbage", "Ada Lovelace"],
    ["auth0|ada", "Ada Lovelace", "Bob Babbage"],
  ]) {
    const search = screen.getByLabelText("Search by name, email or user ID");
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
  const { user, table } = await usersPage();

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
  const { user, table } = await usersPage();

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
  const { user, table } = await usersPage();

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
  const { user, table } = await usersPage();
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
  const { user, table } = await usersPage();

  await user.click(screen.getByRole("button", { name: "Add user" }));
  const dialog = await screen.findByRole("dialog", { name: "Add user" });
  await within(dialog).findByRole("option", { name: "support" });
  await user.type(within(dialog).getByLabelText("User ID"), "  auth0|carol ");
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
  const { table } = await usersPage();

  expect(within(table).getByText("Always admin")).toBeInTheDocument();
  const roles = rolesOf(table, "Root");
  expect(within(roles).getByText("admin")).toBeInTheDocument();
  expect(
    within(table).queryByRole("button", { name: "Remove admin from Root" }),
  ).not.toBeInTheDocument();
  expect(
    within(table).getByRole("button", { name: "Remove support from Root" }),
  ).toBeInTheDocument();
});
