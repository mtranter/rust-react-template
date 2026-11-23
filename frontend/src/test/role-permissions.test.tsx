import { accessApi, grant } from "@/test/access-fixtures";
import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

async function workshop() {
  return screen.findByRole("region", { name: "Workshop" });
}

test("a role's name on the Roles page opens what it may do", async () => {
  server.use(
    ...accessApi([
      grant(1, { role: "support", resource: "module:workshop", action: "use" }),
      grant(2, { role: "support", action: "view", scope: "maker" }),
    ]).handlers,
  );
  const { router } = renderAt("/access/roles");
  const user = userEvent.setup();
  const roles = await screen.findByRole("region", { name: "Roles" });

  await user.click(await within(roles).findByRole("link", { name: "support" }));

  expect(await screen.findByRole("heading", { name: "support" })).toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/access/roles/support");
  const module = await workshop();
  expect(within(module).getByRole("checkbox", { name: "Workshop area: Open" })).toBeChecked();
  expect(within(module).getByRole("combobox", { name: "Gadgets: View" })).toHaveValue("maker");
  expect(within(module).getByRole("combobox", { name: "Gadgets: Make" })).toHaveValue("");
  expect(screen.getByText("Answers customers")).toBeInTheDocument();
});

test("ticking an action grants it, and unticking revokes it at the version listed", async () => {
  const items = [grant(1, { role: "support", resource: "module:workshop", action: "use" })];
  const api = accessApi(items);
  server.use(...api.handlers);
  renderAt("/access/roles/support");
  const user = userEvent.setup();
  const gate = within(await workshop()).getByRole("checkbox", { name: "Workshop area: Open" });

  await user.click(gate);

  await waitFor(() => expect(gate).not.toBeChecked());
  expect(api.ifMatches).toEqual(['"1"']);
  expect(items).toEqual([]);

  await user.click(gate);

  await waitFor(() => expect(gate).toBeChecked());
  expect(items).toMatchObject([
    { role: "support", resource: "module:workshop", action: "use", scope: "any" },
  ]);
});

test("choosing which rows an action reaches grants that, then revokes what it held", async () => {
  const items = [grant(1, { role: "support", action: "view", scope: "any", version: "4" })];
  const api = accessApi(items);
  server.use(...api.handlers);
  renderAt("/access/roles/support");
  const user = userEvent.setup();
  const view = within(await workshop()).getByRole("combobox", { name: "Gadgets: View" });
  expect(view).toHaveValue("any");

  await user.selectOptions(view, "Gadgets they made");

  await waitFor(() => expect(items).toMatchObject([{ action: "view", scope: "maker" }]));
  expect(api.ifMatches).toEqual(['"4"']);
  await waitFor(() => expect(view).toHaveValue("maker"));

  await user.selectOptions(view, "Not allowed");

  await waitFor(() => expect(items).toEqual([]));
});

test("lists a role's grants its modules no longer have, to revoke", async () => {
  server.use(...accessApi([grant(1, { role: "support", action: "polish", stale: true })]).handlers);
  renderAt("/access/roles/support");

  const stale = await screen.findByRole("region", { name: "No longer used" });
  expect(within(stale).getByText(/Gadgets: polish/)).toBeInTheDocument();
  expect(
    within(stale).getByRole("button", {
      name: "Revoke: support can polish Gadgets (Every gadget)",
    }),
  ).toBeInTheDocument();
});

test("reads every page of a role's grants", async () => {
  const many = Array.from({ length: 101 }, (_, i) =>
    grant(i + 1, { role: "support", action: `old${i}`, stale: true }),
  );
  server.use(...accessApi([...many, grant(500, { role: "support", action: "make" })]).handlers);
  renderAt("/access/roles/support");

  const make = within(await workshop()).getByRole("combobox", { name: "Gadgets: Make" });
  expect(make).toHaveValue("any");
});

test("says so for a role that doesn't exist", async () => {
  server.use(...accessApi([]).handlers);
  renderAt("/access/roles/nobody");

  expect(await screen.findByText("There's no role called nobody.")).toBeInTheDocument();
});
