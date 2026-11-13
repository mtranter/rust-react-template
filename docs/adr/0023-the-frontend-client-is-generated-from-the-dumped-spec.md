# The frontend's client is generated from the dumped Spec

The Generated Client is never written by hand. `dump-spec` writes the Spec (ADR 0006) to `frontend/openapi.json`, a checked-in static file, and `@hey-api/openapi-ts` generates `frontend/src/api/` from it with no running server: a typed `@hey-api/client-fetch` client and TanStack Query option and mutation builders, composed with the app's own `useQuery`/`useMutation`. Then `tsc` typechecks. A backend API change that breaks the frontend surfaces as a TypeScript compile error. `make` drives the chain as file targets, and the frontend's typecheck, test and build depend on it, so none runs against a stale client.

Request and response types expose ids, timestamps and versions as plain `string`, though the Domain uses `Uuid` and `chrono`, so the Generated Client needs no date or uuid dependency.

## Considered options

- **Generating from a running server's `/api/spec.json`.** Generation would need the backend, and so a database, up.
- **Hand-written fetch wrappers.** Nothing would keep them in step with the backend.

## Consequences

- hey-api packages are pre-1.0 and pinned exact (ADR 0026).
- A frontend feature that needs another Module's data calls that Module's generated client directly: the API is the contract, as the Module API is on the backend.
