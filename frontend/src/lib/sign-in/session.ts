import {
  ErrorResponse,
  type User,
  type UserManager,
  type UserManagerSettings,
  WebStorageStateStore,
} from "oidc-client-ts";

/** What `GET /auth/config` serves (`crates/server/src/sign_in.rs`), with a client id. */
export type SignInConfig = {
  issuer: string;
  clientId: string;
  scopes: string[];
  extraParams: Record<string, string>;
};

/** The part of `oidc-client-ts`' `UserManager` the SPA uses; tests pass a stub. */
export type SignInProvider = Pick<
  UserManager,
  | "getUser"
  | "signinRedirect"
  | "signinRedirectCallback"
  | "signinSilent"
  | "signoutRedirect"
  | "removeUser"
> & { metadataService: Pick<UserManager["metadataService"], "getEndSessionEndpoint"> };

export const callbackPath = "/auth/callback";
export const signedOutPath = "/auth/signed-out";

/** How long before the access token expires the session renews it. */
export const renewBeforeExpirySeconds = 60;

/**
 * Whether this page is the hidden iframe of a silent renewal, which the provider sends back to the
 * one redirect URI, `/auth/callback`: it hands the response to the app in its parent and shows nothing.
 */
export function isSilentRenewal(win: Window = window): boolean {
  return win.parent !== win && win.location.pathname === callbackPath;
}

/**
 * The SPA's sign-in settings, or `null` when it signs no one in: a `404` (no `APP_JWT_ISSUER`), no
 * client id (dev and test only), or no JSON at all (`make dev`, where Vite answers with the SPA).
 */
export async function fetchSignInConfig(
  fetcher: typeof fetch = fetch,
): Promise<SignInConfig | null> {
  const response = await fetcher("/auth/config", { headers: { accept: "application/json" } });
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`/auth/config answered ${response.status}`);
  if (!response.headers.get("content-type")?.includes("application/json")) return null;
  const config = (await response.json()) as Omit<SignInConfig, "clientId"> & {
    clientId: string | null;
  };
  return config.clientId ? { ...config, clientId: config.clientId } : null;
}

/** Authorization code with PKCE as a public client, its tokens in this tab's `sessionStorage`. */
export function userManagerSettings(config: SignInConfig, origin: string): UserManagerSettings {
  const store = new WebStorageStateStore({ store: window.sessionStorage });
  return {
    authority: config.issuer,
    client_id: config.clientId,
    redirect_uri: `${origin}${callbackPath}`,
    post_logout_redirect_uri: `${origin}${signedOutPath}`,
    response_type: "code",
    scope: config.scopes.join(" "),
    extraQueryParams: config.extraParams,
    userStore: store,
    // Its default is `localStorage`.
    stateStore: store,
    automaticSilentRenew: false,
    monitorSession: false,
  };
}

/** Who is signed in, and the ways in and out. */
export class Session {
  #user: User | null = null;
  #signingIn: Promise<void> | undefined;
  #renewing: Promise<string | undefined> | undefined;
  #renewal: ReturnType<typeof setTimeout> | undefined;
  readonly #callbacks = new Map<string, Promise<string>>();

  private constructor(private readonly provider: SignInProvider) {}

  /** A session with whoever this tab signed in last, if their token hasn't expired. */
  static async restore(provider: SignInProvider): Promise<Session> {
    const session = new Session(provider);
    const user = await provider.getUser();
    session.#signedIn(user && !user.expired ? user : null);
    return session;
  }

  get user(): User | null {
    return this.#user;
  }

  get accessToken(): string | undefined {
    return this.#user?.access_token;
  }

  /** Leaves for the provider's sign-in page, to come back to `returnTo` (a path and search). */
  signIn(returnTo: string): Promise<void> {
    // Once: React's strict mode runs the effect that calls this twice.
    this.#signingIn ??= this.provider.signinRedirect({
      state: { returnTo } satisfies ReturnState,
      // oidc-client-ts sends a nonce, and checks the ID token's, only when given one.
      nonce: randomToken(),
    });
    return this.#signingIn;
  }

  /**
   * Renews the access token, resolving to the new one, or `undefined` when it can't. oidc-client-ts
   * uses the refresh token when the provider issued one, and otherwise the provider's session in a
   * hidden iframe. Callers at the same time share one renewal.
   */
  renew(): Promise<string | undefined> {
    if (!this.#user) return Promise.resolve(undefined);
    this.#renewing ??= this.provider
      .signinSilent()
      .then(
        (user) => {
          if (!user) return undefined;
          this.#signedIn(user, { renewed: true });
          return user.access_token;
        },
        () => undefined,
      )
      .finally(() => {
        this.#renewing = undefined;
      });
    return this.#renewing;
  }

  /**
   * Completes the sign-in the provider redirected back with in `url`, resolving to where the user
   * started; rejects with a {@link SignInError}. Once per URL, as the code can be redeemed only once.
   */
  completeSignIn(url: string): Promise<string> {
    let completing = this.#callbacks.get(url);
    if (!completing) {
      completing = this.provider.signinRedirectCallback(url).then(
        (user) => {
          this.#signedIn(user);
          return returnTo(user.state);
        },
        (error: unknown) => {
          throw signInError(error);
        },
      );
      this.#callbacks.set(url, completing);
    }
    return completing;
  }

  /**
   * Forgets the tokens and, when the provider has an end-session endpoint, leaves to sign out there
   * too (resolving `true`); it sends the browser back to the signed-out page.
   */
  async signOut(): Promise<boolean> {
    const endSession = await this.provider.metadataService
      .getEndSessionEndpoint()
      .catch(() => undefined);
    this.#signedIn(null);
    if (endSession) {
      await this.provider.signoutRedirect();
      return true;
    }
    await this.provider.removeUser();
    return false;
  }

  /** Holds `user`, renewing their access token shortly before it expires. */
  #signedIn(user: User | null, { renewed = false } = {}) {
    this.#user = user;
    clearTimeout(this.#renewal);
    this.#renewal = undefined;
    const expiresIn = user?.expires_in;
    if (expiresIn === undefined) return;
    const delay = Math.max(0, expiresIn - renewBeforeExpirySeconds) * 1000;
    // A renewal that brought a token as short-lived as the margin would renew again at once, forever.
    if (renewed && delay === 0) return;
    // A failed renewal isn't retried here: the request that next gets `401` renews once more.
    this.#renewal = setTimeout(() => void this.renew(), delay);
  }
}

/** A sign-in that failed or was cancelled: the provider's error, and where the user started. */
export class SignInError extends Error {
  constructor(
    message: string,
    readonly returnTo: string,
  ) {
    super(message);
  }
}

type ReturnState = { returnTo: string };

function returnTo(state: unknown): string {
  const path = (state as Partial<ReturnState> | undefined)?.returnTo;
  // A path on this origin only, never `//elsewhere`.
  return typeof path === "string" && path.startsWith("/") && !path.startsWith("//") ? path : "/";
}

function signInError(error: unknown): SignInError {
  if (error instanceof ErrorResponse) {
    const message = error.error_description ?? error.error ?? error.message;
    return new SignInError(message, returnTo(error.state));
  }
  return new SignInError(error instanceof Error ? error.message : String(error), "/");
}

function randomToken(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}
