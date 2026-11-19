import { signedInUser, stubProvider } from "@/test/sign-in-fixtures";
import { http, HttpResponse } from "msw";
import { OidcClient } from "oidc-client-ts";
import { expect, test } from "vitest";
import { server } from "../../../vitest.setup";
import { Session, type SignInConfig, fetchSignInConfig, userManagerSettings } from "./session";

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
