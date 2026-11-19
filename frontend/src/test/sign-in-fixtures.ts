import { Session, type SignInProvider } from "@/lib/sign-in";
import { type SigninRedirectArgs, User } from "oidc-client-ts";
import { vi } from "vitest";

type TokenOptions = { accessToken?: string; expiresIn?: number; refreshToken?: string };

export function signedInUser(
  profile: { name?: string; email?: string } = { name: "Ada Lovelace", email: "ada@example.com" },
  state?: unknown,
  { accessToken = "ada-access-token", expiresIn = 3600, refreshToken }: TokenOptions = {},
): User {
  const now = Math.floor(Date.now() / 1000);
  return new User({
    access_token: accessToken,
    refresh_token: refreshToken,
    token_type: "Bearer",
    expires_at: now + expiresIn,
    profile: {
      sub: "ada",
      iss: "https://id.example.com",
      aud: "spa",
      iat: now,
      exp: now + 3600,
      ...profile,
    },
    userState: state,
  });
}

/**
 * A stand-in for `oidc-client-ts`' `UserManager` that never leaves the page: each redirect is a
 * spy to assert on. `user` is who the tab signed in last; `endSession` the provider's endpoint;
 * `renew` answers a renewal (by default the provider refuses it).
 */
export function stubProvider({
  user = null,
  endSession,
  callback = () => Promise.reject(new Error("no sign-in in progress")),
  renew = () => Promise.reject(new Error("login_required")),
}: {
  user?: User | null;
  endSession?: string;
  callback?: (url: string) => Promise<User>;
  renew?: () => Promise<User | null>;
} = {}) {
  return {
    getUser: vi.fn(async () => user),
    signinRedirect: vi.fn(async (_args?: SigninRedirectArgs) => {}),
    signinRedirectCallback: vi.fn(async (url?: string) => callback(url ?? "")),
    signinSilent: vi.fn(async () => renew()),
    signoutRedirect: vi.fn(async () => {}),
    removeUser: vi.fn(async () => {}),
    metadataService: { getEndSessionEndpoint: vi.fn(async () => endSession) },
  } satisfies SignInProvider;
}

export function sessionOf(provider: SignInProvider): Promise<Session> {
  return Session.restore(provider);
}
