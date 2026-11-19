# Learnings: 0035 SPA sign-in

- Sign-in lives in `frontend/src/lib/sign-in/` (`Session` over a `SignInProvider`, the slice of `oidc-client-ts`' `UserManager` the SPA uses). `main.tsx` builds it from `/auth/config` and passes it to `<App session>`; tests pass `renderAt(path, { session })` a `Session` over `stubProvider()` from `src/test/sign-in-fixtures.ts` (dependency injection, not a module mock). Screens scenarios take `signIn: () => SignInProvider`.
- The routes now hang off a **pathless layout route `app`** (`RootLayout`, which holds the `RequireSignIn` gate), with `/auth/callback` and `/auth/signed-out` beside it under the bare root. So route ids are `/app/<path>`: `getRouteApi("/app/access")`, not `"/access"`. `scripts/init.sh`'s copy of `router.tsx` mirrors this.
- `oidc-client-ts` 3.x sends a `nonce` only when `signinRedirect` is given one; `Session.signIn` passes a random one. Its `stateStore` defaults to `localStorage`, so both stores are set to `sessionStorage` explicitly.
- hey-api's `auth` config option only reaches operations whose spec declares `security`; `src/hey-api.ts` wraps `fetch` instead so every request carries the token. `sendAccessToken` is set from `App` before its first render.
- The callback page must read its URL once (`useState`), not from `useLocation` in the effect's deps: returning to the user's page changes the location and would re-run the code exchange with the wrong URL.
- `RequireSignIn` decides once on mount: after sign-out empties the session, a re-render must not send the user back to the provider.
- Tests that must survive `make init` (sign-in ones) can't visit `/` in the template (it fetches the reference list, which only `src/test/fixtures.ts` answers, and init deletes that); they use `/access`, answered by the default `403` catalog handler.
- In this machine's worktrees port 5432 belongs to another checkout: `make check-changed COMPOSE_PROJECT_NAME=rust-react-template` (and `make check-init` likewise) reuses its Postgres.
- Avoid the whole word `users` in comments of files `make init` keeps ("signs people in"): `check-init` greps for it.
