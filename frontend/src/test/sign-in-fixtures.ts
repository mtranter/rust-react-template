import { Session, type SignInProvider } from "@/lib/sign-in";
import { type SigninRedirectArgs, User } from "oidc-client-ts";
import { vi } from "vitest";

export function signedInUser(
  profile: { name?: string; email?: string } = { name: "Ada Lovelace", email: "ada@example.com" },
  state?: unknown,
): User {
  const now = Math.floor(Date.now() / 1000);
  return new User({
    access_token: "ada-access-token",
    token_type: "Bearer",
    expires_at: now + 3600,
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
 * spy to assert on. `user` is who the tab signed in last; `endSession` the provider's endpoint.
 */
export function stubProvider({
  user = null,
  endSession,
  callback = () => Promise.reject(new Error("no sign-in in progress")),
}: {
  user?: User | null;
  endSession?: string;
  callback?: (url: string) => Promise<User>;
} = {}) {
  return {
    getUser: vi.fn(async () => user),
    signinRedirect: vi.fn(async (_args?: SigninRedirectArgs) => {}),
    signinRedirectCallback: vi.fn(async (url?: string) => callback(url ?? "")),
    signoutRedirect: vi.fn(async () => {}),
    removeUser: vi.fn(async () => {}),
    metadataService: { getEndSessionEndpoint: vi.fn(async () => endSession) },
  } satisfies SignInProvider;
}

export function sessionOf(provider: SignInProvider): Promise<Session> {
  return Session.restore(provider);
}
