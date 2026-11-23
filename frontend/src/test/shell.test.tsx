import { accessApi } from "@/test/access-fixtures";
import { renderAt } from "@/test/render-app";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

// Over the platform's `access` screens, which every app keeps after `make init`.

test("the launcher lists the platform's tools apart, opening one at its first area", async () => {
  server.use(...accessApi([]).handlers);
  const user = userEvent.setup();
  const { router } = renderAt("/");

  const platform = await screen.findByRole("region", { name: "Platform" });
  await user.click(within(platform).getByRole("link", { name: /Access/ }));

  expect(await screen.findByRole("heading", { name: "Users" })).toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/access/subjects");
});

test("the launcher lists no tool the caller may not use", async () => {
  renderAt("/");

  await screen.findByRole("heading", { level: 1 });
  expect(screen.queryByRole("region", { name: "Platform" })).not.toBeInTheDocument();
  expect(screen.queryByRole("link", { name: /Access/ })).not.toBeInTheDocument();
});

test("the sidebar lists the open app's areas, marking the current one, under its breadcrumb", async () => {
  server.use(...accessApi([]).handlers);
  renderAt("/access/roles");

  const areas = await screen.findByRole("navigation", { name: "Access" });
  expect(within(areas).getByRole("link", { name: "Roles" })).toHaveAttribute("data-active");
  expect(within(areas).getByRole("link", { name: "Users" })).not.toHaveAttribute("data-active");

  const crumbs = screen.getByRole("navigation", { name: "breadcrumb" });
  expect(within(crumbs).getByRole("link", { name: "Access" })).toHaveAttribute(
    "href",
    "/access/subjects",
  );
  expect(within(crumbs).getByText("Roles")).toHaveAttribute("aria-current", "page");
});

test("on a record, the breadcrumb's area leads back to the list it is in", async () => {
  server.use(...accessApi([]).handlers);
  renderAt("/access/roles/support");

  expect(await screen.findByRole("heading", { name: "support" })).toBeInTheDocument();
  const crumbs = await screen.findByRole("navigation", { name: "breadcrumb" });
  expect(within(crumbs).getByRole("link", { name: "Roles" })).toHaveAttribute(
    "href",
    "/access/roles",
  );
});

test("the switcher goes back to every app", async () => {
  server.use(...accessApi([]).handlers);
  const user = userEvent.setup();
  const { router } = renderAt("/access/roles");
  await screen.findByRole("region", { name: "Roles" });

  await user.click(screen.getByRole("button", { name: "Switch app" }));
  await user.click(await screen.findByRole("menuitem", { name: "All apps" }));

  expect(await screen.findByRole("region", { name: "Platform" })).toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/");
});

test("Go to jumps to any area of any app, from the keyboard", async () => {
  server.use(...accessApi([]).handlers);
  const user = userEvent.setup();
  const { router } = renderAt("/access/subjects");
  await screen.findByRole("region", { name: "Users" });

  await user.keyboard("{Control>}k{/Control}");
  const dialog = await screen.findByRole("dialog");
  await user.type(within(dialog).getByRole("combobox"), "permissions");
  await user.keyboard("{Enter}");

  expect(await screen.findByRole("heading", { name: "Permissions" })).toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/access/permissions");
});

test("an old link to an access tab goes on to its page, keeping its filter", async () => {
  server.use(...accessApi([]).handlers);
  const { router } = renderAt("/access?tab=grants&role=support");

  expect(await screen.findByRole("heading", { name: "Permissions" })).toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/access/permissions");
  expect(router.state.location.search).toMatchObject({ role: "support" });
});
