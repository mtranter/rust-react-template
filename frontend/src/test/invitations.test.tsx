import {
  accessApi,
  invitation,
  invitationsApi,
  notAnAdmin,
  someInvitations,
} from "@/test/access-fixtures";
import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

function row(table: HTMLElement, email: string) {
  const cell = within(table).getByText(email);
  const found = cell.closest("tr");
  if (!found) throw new Error(`${email} has no row`);
  return found;
}

async function invitationsTab() {
  const user = userEvent.setup();
  const view = renderAt("/access/invitations");
  const table = await screen.findByRole("region", { name: "Invitations" });
  await within(table).findByText("grace@example.com");
  return { user, table, view };
}

async function openInviteDialog(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "Invite user" }));
  const dialog = await screen.findByRole("dialog", { name: "Invite user" });
  await within(dialog).findByRole("checkbox", { name: "support" });
  return dialog;
}

test("lists the invitations on their page, saying where each invitee is", async () => {
  server.use(...accessApi([]).handlers, ...invitationsApi().handlers);
  const { table, view } = await invitationsTab();

  expect(view.router.state.location.pathname).toBe("/access/invitations");
  const grace = row(table, "grace@example.com");
  expect(within(grace).getByText("Pending")).toBeInTheDocument();
  expect(
    within(within(grace).getByRole("list", { name: "Roles of grace@example.com" })).getByText(
      "support",
    ),
  ).toBeInTheDocument();
  expect(
    within(row(table, "hopper@example.com")).getByText("Temporary password expired: resend it"),
  ).toBeInTheDocument();
  const lin = row(table, "lin@example.com");
  expect(within(lin).getByText("Accepted")).toBeInTheDocument();
  expect(within(lin).queryByRole("button")).not.toBeInTheDocument();
});

test("filters the invitations by status, keeping it in the URL", async () => {
  server.use(...accessApi([]).handlers, ...invitationsApi().handlers);
  const { user, table, view } = await invitationsTab();

  await user.selectOptions(within(table).getByLabelText("Status"), "accepted");

  await waitFor(() =>
    expect(within(table).queryByText("grace@example.com")).not.toBeInTheDocument(),
  );
  expect(within(table).getByText("lin@example.com")).toBeInTheDocument();
  expect(view.router.state.location.search).toMatchObject({ status: "accepted" });
});

test("invites someone with the roles chosen, never offering user", async () => {
  server.use(...accessApi([]).handlers, ...invitationsApi().handlers);
  const { user, table } = await invitationsTab();
  const dialog = await openInviteDialog(user);

  expect(within(dialog).queryByRole("checkbox", { name: "user" })).not.toBeInTheDocument();
  await user.type(within(dialog).getByLabelText("Email"), " Ada@Example.com ");
  await user.type(within(dialog).getByLabelText("Name"), "Ada");
  await user.click(within(dialog).getByRole("checkbox", { name: "support" }));
  await user.click(within(dialog).getByRole("button", { name: "Invite" }));

  expect(await screen.findByText("Invitation sent")).toBeInTheDocument();
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  const ada = row(table, "ada@example.com");
  expect(within(ada).getByText("support")).toBeInTheDocument();
  expect(within(ada).getByText("Pending")).toBeInTheDocument();
});

test("an email with an invitation pending isn't invited again: it says so beside the email", async () => {
  server.use(...accessApi([]).handlers, ...invitationsApi().handlers);
  const { user } = await invitationsTab();
  const dialog = await openInviteDialog(user);

  await user.type(within(dialog).getByLabelText("Email"), "grace@example.com");
  await user.click(within(dialog).getByRole("button", { name: "Invite" }));

  expect(await within(dialog).findByText(/pending already/)).toBeInTheDocument();
  expect(within(dialog).getByLabelText("Email")).toHaveAttribute("aria-invalid", "true");
  expect(screen.queryByText("Invitation sent")).not.toBeInTheDocument();
});

test("without an identity provider, inviting says it is off", async () => {
  server.use(...accessApi([]).handlers, ...invitationsApi([], { configured: false }).handlers);
  const user = userEvent.setup();
  renderAt("/access?tab=invitations");
  await screen.findByText("No invitations.");
  const dialog = await openInviteDialog(user);

  await user.type(within(dialog).getByLabelText("Email"), "ada@example.com");
  await user.click(within(dialog).getByRole("button", { name: "Invite" }));

  expect(await within(dialog).findByText(/inviting is off/)).toBeInTheDocument();
});

test("resends a pending invitation once confirmed, sending the version listed", async () => {
  const api = invitationsApi();
  server.use(...accessApi([]).handlers, ...api.handlers);
  const { user, table } = await invitationsTab();

  await user.click(
    within(table).getByRole("button", { name: "Resend the invitation to hopper@example.com" }),
  );
  const confirm = await screen.findByRole("alertdialog", { name: "Resend this invitation?" });
  await user.click(within(confirm).getByRole("button", { name: "Resend" }));

  expect(await screen.findByText("Invitation resent")).toBeInTheDocument();
  expect(api.ifMatches).toEqual(['"1"']);
  await waitFor(() =>
    expect(within(row(table, "hopper@example.com")).queryByText(/expired/)).not.toBeInTheDocument(),
  );
});

test("revokes a pending invitation once confirmed, sending the version listed", async () => {
  const api = invitationsApi();
  server.use(...accessApi([]).handlers, ...api.handlers);
  const { user, table } = await invitationsTab();

  await user.click(
    within(table).getByRole("button", { name: "Revoke the invitation to grace@example.com" }),
  );
  const confirm = await screen.findByRole("alertdialog", { name: "Revoke this invitation?" });
  await user.click(within(confirm).getByRole("button", { name: "Revoke" }));

  expect(await screen.findByText("Invitation revoked")).toBeInTheDocument();
  expect(api.ifMatches).toEqual(['"1"']);
  expect(await within(row(table, "grace@example.com")).findByText("Revoked")).toBeInTheDocument();
});

test("an invitation changed meanwhile isn't revoked: it says so and reloads the list", async () => {
  const invitations = someInvitations();
  server.use(...accessApi([]).handlers, ...invitationsApi(invitations).handlers);
  const { user, table } = await invitationsTab();
  await user.click(
    within(table).getByRole("button", { name: "Revoke the invitation to grace@example.com" }),
  );
  invitations[0] = invitation("grace@example.com", { ...invitations[0], version: "3" });

  const confirm = await screen.findByRole("alertdialog", { name: "Revoke this invitation?" });
  await user.click(within(confirm).getByRole("button", { name: "Revoke" }));

  expect(await screen.findByText(/Someone else changed this invitation/)).toBeInTheDocument();
  expect(screen.queryByText("Invitation revoked")).not.toBeInTheDocument();
});

test("hides the Invitations area from a caller who may not list them, even linked to it", async () => {
  server.use(...notAnAdmin);
  renderAt("/access/invitations");

  expect(await screen.findByText(/You may not manage this/)).toBeInTheDocument();
  expect(screen.queryByRole("navigation", { name: "Access" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Invite user" })).not.toBeInTheDocument();
});
