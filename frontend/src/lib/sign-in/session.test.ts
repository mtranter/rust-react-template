import { signedInUser, stubProvider } from "@/test/sign-in-fixtures";
import { waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { OidcClient, UserManager } from "oidc-client-ts";
import { expect, test, vi } from "vitest";
import { server } from "../../../vitest.setup";
import {
  Session,
  type SignInConfig,
  fetchSignInConfig,
  isSilentRenewal,
  userManagerSettings,
} from "./session";

const config: SignInConfig = {
  issuer: "https://id.example.com",
  clientId: "spa",
  scopes: ["openid", "profile", "email", "offline_access"],
  extraParams: { audience: "https://api.example.com" },
};

const fetchConfig = () =>
  fetchSignInConfig((_, init) => fetch("http://localhost/auth/config", init));

test("signs no one in when the server has no sign-in configured", async () => {
  server.use(
    http.get("*/auth/config", () =>
      HttpResponse.json({ code: "not_found", message: "no", requestId: "t" }, { status: 404 }),
    ),
  );
  expect(await fetchConfig()).toBeNull();
});

test("signs no one in when the dev server answers with the SPA", async () => {
  server.use(http.get("*/auth/config", () => HttpResponse.html("<!doctype html>")));
  expect(await fetchConfig()).toBeNull();
});

test("signs no one in without a client id", async () => {
  server.use(http.get("*/auth/config", () => HttpResponse.json({ ...config, clientId: null })));
  expect(await fetchConfig()).toBeNull();
});

test("reads the sign-in settings", async () => {
  server.use(http.get("*/auth/config", () => HttpResponse.json(config)));
  expect(await fetchConfig()).toEqual(config);
});

test("asks the provider for a code with PKCE, state and nonce, the client id, scopes and extra params", async () => {
  const settings = userManagerSettings(config, "https://app.example.com");
  const client = new OidcClient({
    ...settings,
    metadata: { issuer: config.issuer, authorization_endpoint: `${config.issuer}/authorize` },
  });
  let url = "";
  const provider = stubProvider();
  provider.signinRedirect.mockImplementation(async (args) => {
    url = (await client.createSigninRequest({ ...args })).url;
  });
  const session = await Session.restore(provider);

  await session.signIn("/access");

  const params = new URL(url).searchParams;
  expect(url.startsWith("https://id.example.com/authorize?")).toBe(true);
  expect(params.get("client_id")).toBe("spa");
  expect(params.get("response_type")).toBe("code");
  expect(params.get("redirect_uri")).toBe("https://app.example.com/auth/callback");
  expect(params.get("scope")).toBe("openid profile email offline_access");
  expect(params.get("audience")).toBe("https://api.example.com");
  expect(params.get("code_challenge_method")).toBe("S256");
  expect(params.get("code_challenge")).toBeTruthy();
  expect(params.get("state")).toBeTruthy();
  expect(params.get("nonce")).toMatch(/^[0-9a-f]{32}$/);
  // The PKCE verifier and nonce wait in this tab's sessionStorage, never localStorage.
  expect(Object.keys(sessionStorage).some((key) => key.startsWith("oidc."))).toBe(true);
  expect(Object.keys(localStorage).some((key) => key.startsWith("oidc."))).toBe(false);
  sessionStorage.clear();
});

test("keeps the signed-in user's tokens in sessionStorage", async () => {
  const { userStore } = userManagerSettings(config, "https://app.example.com");
  await userStore?.set("user:spa", "tokens");
  expect(sessionStorage.getItem("oidc.user:spa")).toBe("tokens");
  expect(localStorage.getItem("oidc.user:spa")).toBeNull();
  sessionStorage.clear();
});

test("returns from the callback only to a path on this origin", async () => {
  for (const returnTo of ["//evil.example.com/", "https://evil.example.com/", undefined]) {
    const session = await Session.restore(
      stubProvider({ callback: async () => signedInUser(undefined, { returnTo }) }),
    );
    expect(
      await session.completeSignIn(`https://app.example.com/auth/callback?r=${returnTo}`),
    ).toBe("/");
  }
});

const origin = "https://app.example.com";

/** A real `UserManager` for `config`, its provider's endpoints answered by MSW. */
function userManager() {
  return new UserManager({
    ...userManagerSettings(config, origin),
    metadata: {
      issuer: config.issuer,
      authorization_endpoint: `${config.issuer}/authorize`,
      token_endpoint: `${config.issuer}/token`,
    },
  });
}

/** Records each form the provider's token endpoint is sent, answering with `answer`. */
function tokenEndpoint(answer: Record<string, unknown>) {
  const forms: URLSearchParams[] = [];
  server.use(
    http.post(`${config.issuer}/token`, async ({ request }) => {
      forms.push(new URLSearchParams(await request.text()));
      return HttpResponse.json({ token_type: "Bearer", expires_in: 3600, ...answer });
    }),
  );
  return forms;
}

/** An unsigned ID token: oidc-client-ts reads its claims and leaves its signature to the server. */
function idToken(claims: Record<string, unknown>): string {
  const part = (value: object) => btoa(JSON.stringify(value)).replace(/=+$/, "");
  return `${part({ alg: "none" })}.${part(claims)}.`;
}

/**
 * Silences jsdom's "Not implemented: navigation" error, which it reports when the silent renewal's
 * iframe goes to the provider, on a console of its own that a spy on `console.error` doesn't reach.
 * Returns what puts its reporting back.
 */
type Reporter = (error: Error) => void;
type VirtualConsole = {
  listeners(event: "jsdomError"): Reporter[];
  on(event: "jsdomError", listener: Reporter): void;
  removeAllListeners(event: "jsdomError"): void;
};

function quietNavigation(): () => void {
  const { virtualConsole } = (window as unknown as { jsdom: { virtualConsole: VirtualConsole } })
    .jsdom;
  const reporters = virtualConsole.listeners("jsdomError");
  virtualConsole.removeAllListeners("jsdomError");
  virtualConsole.on("jsdomError", (error: Error) => {
    if (!error.message.startsWith("Not implemented: navigation")) {
      for (const report of reporters) report(error);
    }
  });
  return () => {
    virtualConsole.removeAllListeners("jsdomError");
    for (const report of reporters) virtualConsole.on("jsdomError", report);
  };
}

test("renews the access token with the refresh token when the provider issued one", async () => {
  const forms = tokenEndpoint({ access_token: "renewed", refresh_token: "refresh-2" });
  const manager = userManager();
  await manager.storeUser(signedInUser(undefined, undefined, { refreshToken: "refresh-1" }));
  const session = await Session.restore(manager);

  expect(await session.renew()).toBe("renewed");

  expect(session.accessToken).toBe("renewed");
  expect(forms).toHaveLength(1);
  expect(forms[0].get("grant_type")).toBe("refresh_token");
  expect(forms[0].get("refresh_token")).toBe("refresh-1");
  expect(forms[0].get("client_id")).toBe("spa");
  // A refreshed session still names the user its ID token did.
  expect(session.user?.profile.name).toBe("Ada Lovelace");
});

test("with no refresh token, renews silently through the provider's session, back at /auth/callback", async () => {
  const now = Math.floor(Date.now() / 1000);
  const forms = tokenEndpoint({
    access_token: "renewed",
    id_token: idToken({ iss: config.issuer, aud: "spa", sub: "ada", iat: now, exp: now + 3600 }),
  });
  const manager = userManager();
  await manager.storeUser(signedInUser());
  const session = await Session.restore(manager);
  const unquiet = quietNavigation();
  try {
    const renewing = session.renew();

    // The hidden iframe asks the provider with `prompt=none`, to come back to the one redirect URI.
    let stateId = "";
    await waitFor(() => {
      const stored = Object.keys(sessionStorage)
        .filter((key) => key.startsWith("oidc.") && !key.startsWith("oidc.user:"))
        .map((key) => JSON.parse(sessionStorage.getItem(key) ?? "{}"));
      const silent = stored.find((state) => state.request_type === "si:s");
      expect(silent?.redirect_uri).toBe(`${origin}/auth/callback`);
      stateId = silent.id;
    });
    expect(document.querySelector("iframe")).not.toBeNull();
    // What `/auth/callback` in that iframe (`isSilentRenewal`) posts back once the provider answers.
    window.dispatchEvent(
      new MessageEvent("message", {
        origin: window.location.origin,
        data: {
          source: "oidc-client",
          url: `${origin}/auth/callback?code=the-code&state=${stateId}`,
          keepOpen: false,
        },
      }),
    );

    expect(await renewing).toBe("renewed");
    expect(session.accessToken).toBe("renewed");
    expect(forms[0].get("grant_type")).toBe("authorization_code");
    expect(forms[0].get("redirect_uri")).toBe(`${origin}/auth/callback`);
    expect(forms[0].get("code_verifier")).toBeTruthy();
  } finally {
    unquiet();
  }
});

test("renews a minute before the access token expires, and again before the new one does", async () => {
  vi.useFakeTimers();
  try {
    const provider = stubProvider({
      user: signedInUser(undefined, undefined, { expiresIn: 120 }),
      renew: async () =>
        signedInUser(undefined, undefined, { accessToken: "renewed", expiresIn: 300 }),
    });
    const session = await Session.restore(provider);

    await vi.advanceTimersByTimeAsync(59_000);
    expect(provider.signinSilent).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(provider.signinSilent).toHaveBeenCalledOnce();
    expect(session.accessToken).toBe("renewed");

    await vi.advanceTimersByTimeAsync(240_000);
    expect(provider.signinSilent).toHaveBeenCalledTimes(2);
  } finally {
    vi.useRealTimers();
  }
});

test("doesn't renew again at once when a renewal brings a token about to expire", async () => {
  vi.useFakeTimers();
  try {
    const provider = stubProvider({
      user: signedInUser(undefined, undefined, { expiresIn: 30 }),
      renew: async () => signedInUser(undefined, undefined, { expiresIn: 30 }),
    });
    await Session.restore(provider);

    await vi.advanceTimersByTimeAsync(10_000);
    expect(provider.signinSilent).toHaveBeenCalledOnce();
  } finally {
    vi.useRealTimers();
  }
});

test("shares one renewal between callers, and resolves to nothing when the provider refuses", async () => {
  const provider = stubProvider({ user: signedInUser() });
  const session = await Session.restore(provider);

  expect(await Promise.all([session.renew(), session.renew()])).toEqual([undefined, undefined]);
  expect(provider.signinSilent).toHaveBeenCalledOnce();
  expect(session.accessToken).toBe("ada-access-token");
});

test("the silent renewal's callback is /auth/callback in a frame, and only there", () => {
  const page = (pathname: string, framed: boolean) => {
    const win = { location: { pathname } } as unknown as Window & { parent: Window };
    win.parent = framed ? ({} as Window) : win;
    return win;
  };
  expect(isSilentRenewal(page("/auth/callback", true))).toBe(true);
  expect(isSilentRenewal(page("/auth/callback", false))).toBe(false);
  expect(isSilentRenewal(page("/access", true))).toBe(false);
});

test("a renewal that finishes after signing out doesn't sign the user back in", async () => {
  let finish: (user: ReturnType<typeof signedInUser>) => void = () => {};
  const provider = stubProvider({
    user: signedInUser(),
    renew: () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  });
  const session = await Session.restore(provider);

  const renewing = session.renew();
  await session.signOut();
  finish(signedInUser(undefined, undefined, { accessToken: "renewed" }));

  expect(await renewing).toBeUndefined();
  expect(session.user).toBeNull();
});

test("signs in again after a refused token only until the API accepts one", async () => {
  const provider = stubProvider({ user: signedInUser() });

  expect(await (await Session.restore(provider)).signInAgain("/access")).toBe(true);
  // Back from the provider, in a new page, with a token the API refuses too.
  const back = await Session.restore(provider);
  expect(await back.signInAgain("/access")).toBe(false);
  expect(provider.signinRedirect).toHaveBeenCalledOnce();

  back.accepted();
  expect(await back.signInAgain("/access")).toBe(true);
  expect(provider.signinRedirect).toHaveBeenCalledTimes(2);
});
