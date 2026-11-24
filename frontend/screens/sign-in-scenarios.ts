import type { SignInProvider } from "@/lib/sign-in";
import { http, HttpResponse } from "msw";
import { ErrorResponse, User } from "oidc-client-ts";
import type { Scenario } from "./scenarios.ts";

// The platform's sign-in screens, which `make init` keeps. Like scenarios.ts, run by Node's type
// stripping: so only package imports, and types from `@/`.

/** A provider that never leaves the page: `user` is who this tab signed in last. */
function provider({
  user = null,
  callback = () => Promise.reject(new Error("no sign-in in progress")),
}: { user?: User | null; callback?: () => Promise<User> } = {}): SignInProvider {
  return {
    getUser: async () => user,
    signinRedirect: async () => {},
    signinRedirectCallback: callback,
    signinSilent: async () => null,
    signoutRedirect: async () => {},
    removeUser: async () => {},
    metadataService: { getEndSessionEndpoint: async () => undefined },
  };
}

function ada(): User {
  const now = Math.floor(Date.now() / 1000);
  return new User({
    access_token: "screens",
    token_type: "Bearer",
    expires_at: now + 3600,
    profile: {
      sub: "ada",
      iss: "https://id.example.com",
      aud: "spa",
      iat: now,
      exp: now + 3600,
      name: "Ada Lovelace",
    },
  });
}

export const signInScenarios: Scenario[] = [
  {
    name: "signed-in",
    route: "/access/subjects",
    // As to a user who isn't an admin, whom the page's list refuses.
    handlers: [
      http.get("*/api/access/subjects", () =>
        HttpResponse.json(
          { code: "forbidden", message: "forbidden", requestId: "screens" },
          { status: 403 },
        ),
      ),
    ],
    signIn: () => provider({ user: ada() }),
    steps: async (page) => {
      await page.getByRole("button", { name: "Account: Ada Lovelace" }).click();
    },
    shows: (page) => page.getByRole("menuitem", { name: "Sign out" }),
  },
  {
    name: "sign-in-failed",
    route: "/auth/callback?error=access_denied&state=screens",
    handlers: [],
    signIn: () =>
      provider({
        callback: () =>
          Promise.reject(
            new ErrorResponse({
              error: "access_denied",
              error_description: "The user cancelled the sign-in.",
            }),
          ),
      }),
    shows: (page) => page.getByRole("button", { name: "Try again" }),
  },
  {
    name: "signed-out",
    route: "/auth/signed-out",
    handlers: [],
    signIn: () => provider(),
    shows: (page) => page.getByRole("link", { name: "Sign in again" }),
  },
];
