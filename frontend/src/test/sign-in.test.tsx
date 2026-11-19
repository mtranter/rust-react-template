import { renderAt } from "@/test/render-app";
import { sessionOf, signedInUser, stubProvider } from "@/test/sign-in-fixtures";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { ErrorResponse } from "oidc-client-ts";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

/** Records the `Authorization` header of each catalog request, answering as to a non-admin. */
function catalogAuthorizations() {
  const seen: (string | null)[] = [];
  server.use(
    http.get("*/api/access/catalog", ({ request }) => {
      seen.push(request.headers.get("authorization"));
      return HttpResponse.json(
        { code: "forbidden", message: "forbidden", requestId: "test" },
        { status: 403 },
      );
    }),
  );
  return seen;
}

test("with no sign-in configured, shows the app and sends no token of its own", async () => {
  const seen = catalogAuthorizations();
  renderAt("/access");

  expect(await screen.findByText(/Only an admin can manage/)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Sign out" })).not.toBeInTheDocument();
  await waitFor(() => expect(seen).toEqual([null]));
});

test("sends a visitor who isn't signed in straight to the provider, to come back to the same page", async () => {
  const seen = catalogAuthorizations();
  const provider = stubProvider();
  renderAt("/access?tab=roles", { session: await sessionOf(provider) });

  await waitFor(() => expect(provider.signinRedirect).toHaveBeenCalledOnce());
  expect(provider.signinRedirect).toHaveBeenCalledWith({
    state: { returnTo: "/access?tab=roles" },
    nonce: expect.stringMatching(/^[0-9a-f]{32}$/),
  });
  expect(screen.queryByRole("heading", { name: "Access" })).not.toBeInTheDocument();
  expect(seen).toEqual([]);
});

test("treats an expired session as signed out", async () => {
  const expired = signedInUser();
  expired.expires_at = Math.floor(Date.now() / 1000) - 60;
  const provider = stubProvider({ user: expired });
  renderAt("/", { session: await sessionOf(provider) });

  await waitFor(() => expect(provider.signinRedirect).toHaveBeenCalledOnce());
});

test("sends the signed-in user's access token on every request", async () => {
  const seen = catalogAuthorizations();
  renderAt("/access", { session: await sessionOf(stubProvider({ user: signedInUser() })) });

  expect(await screen.findByText(/Only an admin can manage/)).toBeInTheDocument();
  await waitFor(() => expect(seen).toEqual(["Bearer ada-access-token"]));
});

test("completes the sign-in at the callback and returns to the page the user started on", async () => {
  const seen = catalogAuthorizations();
  const provider = stubProvider({
    callback: async () => signedInUser(undefined, { returnTo: "/access?tab=roles" }),
  });
  const { router } = renderAt("/auth/callback?code=the-code&state=the-state", {
    session: await sessionOf(provider),
  });

  await waitFor(() => expect(router.state.location.href).toBe("/access?tab=roles"));
  expect(provider.signinRedirectCallback).toHaveBeenCalledOnce();
  expect(provider.signinRedirectCallback).toHaveBeenCalledWith(
    expect.stringMatching(/\/auth\/callback\?code=the-code&state=the-state$/),
  );
  expect(await screen.findByText("Ada Lovelace")).toBeInTheDocument();
  await waitFor(() => expect(seen).toEqual(["Bearer ada-access-token"]));
  expect(provider.signinRedirect).not.toHaveBeenCalled();
});

test("shows the provider's error when sign-in fails, and signs in again only when asked", async () => {
  const provider = stubProvider({
    callback: () =>
      Promise.reject(
        new ErrorResponse({
          error: "access_denied",
          error_description: "The user cancelled the sign-in",
          userState: { returnTo: "/access?tab=roles" },
        }),
      ),
  });
  const user = userEvent.setup();
  renderAt("/auth/callback?error=access_denied&state=s", { session: await sessionOf(provider) });

  expect(
    await screen.findByRole("heading", { name: "Sign-in didn't complete" }),
  ).toBeInTheDocument();
  expect(screen.getByText("The user cancelled the sign-in")).toBeInTheDocument();
  expect(provider.signinRedirect).not.toHaveBeenCalled();

  await user.click(screen.getByRole("button", { name: "Try again" }));

  expect(provider.signinRedirect).toHaveBeenCalledWith(
    expect.objectContaining({ state: { returnTo: "/access?tab=roles" } }),
  );
});

test("the signed-out page stays until the user signs in again", async () => {
  const provider = stubProvider();
  const user = userEvent.setup();
  renderAt("/auth/signed-out", { session: await sessionOf(provider) });

  expect(await screen.findByRole("heading", { name: "You're signed out" })).toBeInTheDocument();
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(provider.signinRedirect).not.toHaveBeenCalled();

  await user.click(screen.getByRole("link", { name: "Sign in again" }));

  await waitFor(() => expect(provider.signinRedirect).toHaveBeenCalledOnce());
  expect(provider.signinRedirect).toHaveBeenCalledWith(
    expect.objectContaining({ state: { returnTo: "/" } }),
  );
});

test("the header names the signed-in user, by email when the ID token has no name", async () => {
  catalogAuthorizations();
  renderAt("/access", {
    session: await sessionOf(stubProvider({ user: signedInUser({ email: "ada@example.com" }) })),
  });

  expect(await screen.findByText("ada@example.com")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Sign out" })).toBeInTheDocument();
});

test("signing out forgets the tokens and ends on the signed-out page", async () => {
  catalogAuthorizations();
  const provider = stubProvider({ user: signedInUser() });
  const user = userEvent.setup();
  const { router } = renderAt("/access", { session: await sessionOf(provider) });

  await user.click(await screen.findByRole("button", { name: "Sign out" }));

  expect(await screen.findByRole("heading", { name: "You're signed out" })).toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/auth/signed-out");
  expect(provider.removeUser).toHaveBeenCalledOnce();
  expect(provider.signoutRedirect).not.toHaveBeenCalled();
  expect(provider.signinRedirect).not.toHaveBeenCalled();
});

test("signing out also signs out at the provider when it has an end-session endpoint", async () => {
  catalogAuthorizations();
  const provider = stubProvider({
    user: signedInUser(),
    endSession: "https://id.example.com/logout",
  });
  const user = userEvent.setup();
  renderAt("/access", { session: await sessionOf(provider) });

  await user.click(await screen.findByRole("button", { name: "Sign out" }));

  await waitFor(() => expect(provider.signoutRedirect).toHaveBeenCalledOnce());
  expect(provider.signinRedirect).not.toHaveBeenCalled();
});
