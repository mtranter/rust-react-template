import { accessApi, adminPermissions, notAnAdmin } from "@/test/access-fixtures";
import { renderAt } from "@/test/render-app";
import { sessionOf, signedInUser, stubProvider } from "@/test/sign-in-fixtures";
import { cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { ErrorResponse } from "oidc-client-ts";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

/**
 * Records the `Authorization` header of each request for the caller's permissions, answering as
 * to a non-admin, whom `access` refuses.
 */
function permissionsAuthorizations() {
  const seen: (string | null)[] = [];
  server.use(
    http.get("*/api/access/me/permissions", ({ request }) => {
      seen.push(request.headers.get("authorization"));
      return HttpResponse.json({ permissions: [] });
    }),
    ...notAnAdmin,
  );
  return seen;
}

test("with no sign-in configured, shows the app and sends no token of its own", async () => {
  const seen = permissionsAuthorizations();
  renderAt("/access");

  expect(await screen.findByText(/You may not manage this/)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /^Account/ })).not.toBeInTheDocument();
  await waitFor(() => expect(seen).toEqual([null]));
});

test("sends a visitor who isn't signed in straight to the provider, to come back to the same page", async () => {
  const seen = permissionsAuthorizations();
  const provider = stubProvider();
  renderAt("/access/roles", { session: await sessionOf(provider) });

  await waitFor(() => expect(provider.signinRedirect).toHaveBeenCalledOnce());
  expect(provider.signinRedirect).toHaveBeenCalledWith({
    state: { returnTo: "/access/roles" },
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
  const seen = permissionsAuthorizations();
  renderAt("/access", { session: await sessionOf(stubProvider({ user: signedInUser() })) });

  expect(await screen.findByText(/You may not manage this/)).toBeInTheDocument();
  await waitFor(() => expect(seen).toEqual(["Bearer ada-access-token"]));
});

test("completes the sign-in at the callback and returns to the page the user started on", async () => {
  const seen = permissionsAuthorizations();
  const provider = stubProvider({
    callback: async () => signedInUser(undefined, { returnTo: "/access/roles" }),
  });
  const { router } = renderAt("/auth/callback?code=the-code&state=the-state", {
    session: await sessionOf(provider),
  });

  await waitFor(() => expect(router.state.location.href).toBe("/access/roles"));
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
          userState: { returnTo: "/access/roles" },
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
    expect.objectContaining({ state: { returnTo: "/access/roles" } }),
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

test("the header names the signed-in user, by email when the ID token has no name, with a way out", async () => {
  permissionsAuthorizations();
  renderAt("/access", {
    session: await sessionOf(stubProvider({ user: signedInUser({ email: "ada@example.com" }) })),
  });

  const user = userEvent.setup();
  const account = await screen.findByRole("button", { name: "Account: ada@example.com" });
  expect(account).toHaveTextContent("ada@example.com");

  await user.click(account);
  expect(await screen.findByRole("menuitem", { name: "Sign out" })).toBeInTheDocument();
});

test("signing out forgets the tokens and ends on the signed-out page", async () => {
  permissionsAuthorizations();
  const provider = stubProvider({ user: signedInUser() });
  const user = userEvent.setup();
  const { router } = renderAt("/access", { session: await sessionOf(provider) });

  await user.click(await screen.findByRole("button", { name: /^Account/ }));
  await user.click(await screen.findByRole("menuitem", { name: "Sign out" }));

  expect(await screen.findByRole("heading", { name: "You're signed out" })).toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/auth/signed-out");
  expect(provider.removeUser).toHaveBeenCalledOnce();
  expect(provider.signoutRedirect).not.toHaveBeenCalled();
  expect(provider.signinRedirect).not.toHaveBeenCalled();
});

test("signing out also signs out at the provider when it has an end-session endpoint", async () => {
  permissionsAuthorizations();
  const provider = stubProvider({
    user: signedInUser(),
    endSession: "https://id.example.com/logout",
  });
  const user = userEvent.setup();
  renderAt("/access", { session: await sessionOf(provider) });

  await user.click(await screen.findByRole("button", { name: /^Account/ }));
  await user.click(await screen.findByRole("menuitem", { name: "Sign out" }));

  await waitFor(() => expect(provider.signoutRedirect).toHaveBeenCalledOnce());
  expect(provider.signinRedirect).not.toHaveBeenCalled();
});

/**
 * The access screen's API for an admin, authorizing only `valid` tokens (`401` otherwise); records
 * the `Authorization` header of each request for the caller's permissions.
 */
function adminFor(valid: string[]) {
  const seen: (string | null)[] = [];
  const answer = (request: Request, body: object) =>
    valid.some((token) => request.headers.get("authorization") === `Bearer ${token}`)
      ? HttpResponse.json(body)
      : HttpResponse.json(
          { code: "unauthenticated", message: "unauthenticated", requestId: "test" },
          { status: 401 },
        );
  server.use(
    http.get("*/api/access/me/permissions", ({ request }) => {
      seen.push(request.headers.get("authorization"));
      return answer(request, { permissions: adminPermissions });
    }),
    // The header asks too, and the API refuses its token alike.
    http.get("*/api/assistant/status", ({ request }) => answer(request, { on: false })),
    ...accessApi([]).handlers,
  );
  return seen;
}

const renewedUser = () =>
  signedInUser(undefined, undefined, { accessToken: "renewed-access-token" });

test("a 401 renews the token once and retries the request with the new one", async () => {
  const seen = adminFor(["renewed-access-token"]);
  const provider = stubProvider({ user: signedInUser(), renew: async () => renewedUser() });
  renderAt("/access/roles", { session: await sessionOf(provider) });

  expect(await screen.findByRole("region", { name: "Roles" })).toBeInTheDocument();
  expect(seen).toEqual(["Bearer ada-access-token", "Bearer renewed-access-token"]);
  expect(provider.signinSilent).toHaveBeenCalledOnce();
  expect(provider.signinRedirect).not.toHaveBeenCalled();
});

test("when renewal fails after a 401, sends the user to sign in, to come back to the same page", async () => {
  const seen = adminFor([]);
  const provider = stubProvider({ user: signedInUser() });
  renderAt("/access/roles", { session: await sessionOf(provider) });

  await waitFor(() => expect(provider.signinRedirect).toHaveBeenCalledOnce());
  expect(provider.signinRedirect).toHaveBeenCalledWith(
    expect.objectContaining({ state: { returnTo: "/access/roles" } }),
  );
  expect(provider.signinSilent).toHaveBeenCalledOnce();
  // No retry without a new token, and nothing more after leaving to sign in.
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(seen).toEqual(["Bearer ada-access-token"]);
});

test("when the retry gets a 401 too, sends the user to sign in rather than renewing again", async () => {
  const seen = adminFor([]);
  const provider = stubProvider({ user: signedInUser(), renew: async () => renewedUser() });
  renderAt("/access/subjects", { session: await sessionOf(provider) });

  await waitFor(() => expect(provider.signinRedirect).toHaveBeenCalledOnce());
  expect(provider.signinRedirect).toHaveBeenCalledWith(
    expect.objectContaining({ state: { returnTo: "/access/subjects" } }),
  );
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(provider.signinSilent).toHaveBeenCalledOnce();
  expect(seen).toEqual(["Bearer ada-access-token", "Bearer renewed-access-token"]);
});

test("back from a sign-in whose token the API refuses too, shows the refusal rather than signing in again", async () => {
  const seen = adminFor([]);
  // The page's own list is refused too, so the refusal has somewhere to show.
  server.use(
    http.get("*/api/access/subjects", () =>
      HttpResponse.json(
        { code: "unauthenticated", message: "unauthenticated", requestId: "test" },
        { status: 401 },
      ),
    ),
  );
  const before = stubProvider({ user: signedInUser() });
  renderAt("/access", { session: await sessionOf(before) });
  await waitFor(() => expect(before.signinRedirect).toHaveBeenCalledOnce());
  cleanup();

  // The provider signed the user straight back in, and the app starts again in the same tab.
  const after = stubProvider({ user: signedInUser() });
  renderAt("/access", { session: await sessionOf(after) });

  expect(await screen.findByText("Failed to load the subjects.")).toBeInTheDocument();
  // Each refused request tries a renewal of its own once the last has ended; none signs in.
  expect(after.signinSilent).toHaveBeenCalled();
  expect(after.signinRedirect).not.toHaveBeenCalled();
  expect(seen).toHaveLength(2);
});

test("a 403 neither renews nor signs in: the access screen just hides", async () => {
  const seen = permissionsAuthorizations();
  const provider = stubProvider({ user: signedInUser(), renew: async () => renewedUser() });
  renderAt("/access", { session: await sessionOf(provider) });

  expect(await screen.findByText(/You may not manage this/)).toBeInTheDocument();
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(seen).toEqual(["Bearer ada-access-token"]);
  expect(provider.signinSilent).not.toHaveBeenCalled();
  expect(provider.signinRedirect).not.toHaveBeenCalled();
});

test("with no sign-in configured, a 401 is neither renewed nor retried", async () => {
  const seen = adminFor([]);
  renderAt("/access");

  await waitFor(() => expect(seen).toEqual([null]));
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(seen).toEqual([null]);
});
