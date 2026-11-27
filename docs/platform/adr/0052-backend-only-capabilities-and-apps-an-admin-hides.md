# Backend-only Capabilities, and Apps an admin hides

A Capability can be wanted for its API alone: a backend that other systems, or agents through `/mcp`, call, with nothing for a person to see. And an organisation may not want everyone to see an App that is built, for now or for good, without removing its Module. ADR 0047 gave each Capability one App and made `make check-modules` fail on one without, apart from `assistant`, hard-coded in the script.

**A Capability says in its `capability.yaml` whether it has an App.** `app: false` makes it backend-only: no entry in `frontend/src/shell/capabilities.ts`, and `make check-modules` fails on one that has an entry anyway. Its operations, and their MCP tools, are as any Capability's. `make new-module .. APP=false` makes a new Capability this way. `assistant` says `app: false`, replacing the script's list.

**An admin hides Apps at runtime, for everyone.** The platform's UI settings Module, `ui-settings` (until now `appearance`, renamed as it now holds more than the Theme), stores the ids of the hidden Apps, one root keyed `org`, versioned and announced on `ui_settings.app_visibility.v1` like the Theme (ADR 0051). `get_app_visibility` is `SIGNED_IN`: every shell reads it, and it holds only ids. `update_app_visibility` takes `If-Match` and is guarded by `appVisibility` / `update`, `admin`'s by default grant. The shell leaves a hidden App out of the launcher, the sidebar and Go to, as it leaves out an App whose Areas the caller may not use (ADR 0046), and waits for the answer before drawing any. Admin › UI settings › App visibility lists every App with a switch.

**Hiding is only hiding.** A hidden App's URLs still route, and its operations and tools still answer, under the same grants. Withholding them is what the grants are for (ADR 0040). The platform's tools (`access`, `connections`, `ui-settings`) can't be hidden, so no admin hides their own way back.

## Considered options

- **Enforce it on the server.** Turning a Capability's operations and tools off would need a check in the request path and a second way, beside grants, to refuse an operation. Revoking a Capability's grants already does that.
- **A `hidden` flag on registry entries.** It needs a rebuild to change, and it is a second way to say what `app: false` says.
- **A Module of its own (`apps`).** It would be one small root. `ui-settings` already holds the org's say over how the SPA looks.
- **Check the ids against the registered Capabilities.** The SPA's registry, not the server, says which Apps there are. An id it doesn't know hides nothing, and one left over from a removed App is harmless.

## Consequences

- An App removed from the registry may leave its id in the stored list. That has no effect until an App takes the id again.
- `appearance` is renamed `ui-settings` throughout: crates, schema `ui_settings` (`ALTER SCHEMA appearance RENAME TO ui_settings` on a database that ran its migrations), topics `ui_settings.theme.v1` and `ui_settings.app_visibility.v1`, the API under `/api/ui-settings/`, and the SPA's tool at `/ui-settings`, with Theming and App visibility areas. `/appearance` redirects to Theming. Nothing had been released under the old names.
- The shell makes one more request before it draws anything, cached for a minute like the caller's permissions.
- A test or scenario of a hidden App answers `get_app_visibility` itself. By default none is hidden.
