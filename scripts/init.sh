#!/usr/bin/env bash
# Turns a fresh clone of the template into a greenfield app, once, in one
# commit: its own name and domain docs, the reference modules registered with
# no server, the UI down to a shell. The platform's modules (`crates/platform/`,
# `access`, `assistant`, `connections` and `appearance`) stay registered: they are the template's, not reference code. The reference code stays, compiled and
# tested, for agents to copy until the app's ticket 0001 retires it. Then it
# removes itself, its make targets and its CI job.
#
#   scripts/init.sh <name> "<title>"    e.g. `crm "Acme CRM"` (`make init NAME=.. TITLE=..`)
#
# Refuses on a dirty tree, or once TEMPLATE_VERSION exists. Ends with `make
# check`. If that or any step before it fails, the tree goes back to how it was,
# so `make init` can run again, and what it had changed goes to a stash.
set -euo pipefail
cd "$(dirname "$0")/.."

die() {
  echo "init: $1" >&2
  exit 1
}

name=${1:-}
title=${2:-}
[ -n "$name" ] && [ -n "$title" ] || die 'usage: make init NAME=<name> TITLE="<display name>"'
[[ $name =~ ^[a-z][a-z0-9]*(-[a-z0-9]+)*$ ]] || die "\`$name\`: use lowercase words joined by '-'"
# Written into Rust, HTML, JSON, TSX and Markdown as it is, so nothing to escape.
[[ $title =~ ^[A-Za-z0-9][A-Za-z0-9\ .\&\'-]*$ ]] ||
  die "\`$title\`: use letters, digits, spaces and . & ' - only"
[ ! -e TEMPLATE_VERSION ] || die "TEMPLATE_VERSION exists: this is an app already"
[ -z "$(git status --porcelain)" ] || die "the working tree has changes: commit or stash them first"

commit=$(git rev-parse HEAD)
short=$(git rev-parse --short HEAD)
remote=$(git remote get-url origin 2>/dev/null || true)

# Where the template's files can be read at $commit, for the UI's reference
# feature this removes.
upstream=
if [[ $remote =~ ^(https://|git@|ssh://git@)github\.com[:/]([^/]+/[^/]+)$ ]]; then
  upstream="https://github.com/${BASH_REMATCH[2]%.git}/blob/$commit"
fi

export NAME=$name TITLE=$title COMMIT=$commit

echo "init: $title ($name) from the template at $short"
make --no-print-directory setup

# A half-made app can't be initialised again: TEMPLATE_VERSION exists, the tree
# is dirty and the Makefile has no `init`.
put_back() {
  local status=$?
  trap - EXIT
  if [ -n "$(git status --porcelain)" ]; then
    if git stash push -q -u -m "make init, failed" 2>/dev/null; then
      echo "init: failed. The tree is as it was; what init had changed is in the stash (\`git stash show -p\`, \`git stash drop\`)." >&2
    else
      git reset -q --hard
      git clean -qfd
      echo "init: failed. The tree is as it was." >&2
    fi
  fi
  exit "$status"
}
trap put_back EXIT

# 1. The platform this app was cut from, for pulling its later changes.
cat >TEMPLATE_VERSION <<EOF
commit $commit
remote ${remote:-none}
EOF

# 2. The business domain's docs, beside the platform's (docs/agents/domain.md).
cat >GLOSSARY.md <<EOF
# $title

The business domain's terms: what $title is about, each with one meaning across the code and conversation. The platform's terms (Module, Aggregate Root, Topic, Projection…) are in \`docs/platform/glossary.md\`; \`docs/agents/domain.md\` says how the two are used.

## Language

## Relationships

## Flagged ambiguities
EOF
mkdir -p docs/adr docs/tickets
touch docs/adr/.gitkeep
cat >docs/tickets/0001-retire-the-reference-modules.md <<EOF
---
id: 0001
name: The reference code is retired once the app's own code shows every pattern it showed
status: needs-triage
priority: P2
size: M
blocked_by: []
blocks: []
source: \`make init\`, $(date +%Y-%m-%d)
---

# The reference code is retired once the app's own code shows every pattern it showed

## Description

The template's reference modules stayed behind in \`crates/reference/\` when this app was cut from it, compiled and tested but registered with no server, because the docs agents follow point at them: \`crates/AGENTS.md\`'s "Copy the reference code" table, and \`crates/reference/journeys/\` for the journeys over them. The UI's reference feature was removed; \`frontend/AGENTS.md\` links to it at the template commit in \`TEMPLATE_VERSION\`.

Once this app's own code shows the same patterns, the reference code is dead weight: copy the app's instead.

## Acceptance criteria

- [ ] The app's own modules cover each row of \`crates/AGENTS.md\`'s table: a write module with an aggregate root with children; a module API; a port with an adapter; calling another module; reacting to another module's events; a projection; a read module; retries with an \`Idempotency-Key\`; a cross-module journey in \`crates/server/tests/journeys/\` over the registered modules.
- [ ] The app's first feature covers \`frontend/AGENTS.md\`'s: a server-paged list with its page and sort in the URL, a create form showing server field errors, an edit dialog sending \`If-Match\` and handling a \`412\`.
- [ ] Both tables, \`docs/agents/domain.md\` and \`frontend/AGENTS.md\`'s links point at the app's code instead.
- [ ] The platform journeys in \`crates/reference/journeys/\` (\`agents\`, \`compacting_history\`, \`connecting\`) are rewritten over the app's modules in \`crates/server/tests/journeys/\`, so the platform keeps its coverage.
- [ ] \`crates/platform/\` stays: \`access\`, \`assistant\`, \`connections\` and \`appearance\` are the platform's, registered in every app, not reference code.
- [ ] \`crates/reference/\`, its workspace members and dependencies in the root \`Cargo.toml\`, its packages in \`deny.toml\`, its queries in \`.sqlx\` (\`make prepare\`) and its mentions in \`scripts/\` are gone, and \`make check\` passes.
EOF

# 3. The app's name.
perl -i -pe 's{^pub const API_TITLE: &str = ".*";}{pub const API_TITLE: &str = "$ENV{TITLE} API";}' crates/server/src/lib.rs
perl -i -pe 's{^pub const APP_NAME: &str = ".*";}{pub const APP_NAME: &str = "$ENV{NAME}";}' crates/server/src/lib.rs
perl -i -pe 's{<title>.*</title>}{<title>$ENV{TITLE}</title>}' frontend/index.html
perl -i -pe 's{^  "name": ".*",}{  "name": "$ENV{NAME}",}' frontend/package.json
perl -0 -i -pe '
  s{\A# AGENTS\.md\n\n.*?\n\n}{# AGENTS.md\n\n**$ENV{TITLE}**: a Rust modular-monolith backend (Poem + poem-openapi) whose OpenAPI spec drives a type-safe Vite + React + TypeScript frontend. Started from a template, at the commit `TEMPLATE_VERSION` records; the platform it brought is described in `docs/platform/`.\n\n}s;
  s{^- `make init .*\n}{}m;
' AGENTS.md

# 4. The reference modules off the server: compiled and tested, registered nowhere.
# The platform's `access` stays, granting from whatever the app registers, and
# so do its `assistant`, `connections` and `appearance`.
cat >crates/server/src/modules.rs <<'EOF'
//! The one place a module is registered: its `Endpoints` in the tuple and its
//! `module()` in the list (`make new-module` adds both; `make check-modules`
//! fails on a module missing from either). The platform's `access` comes first:
//! it grants from every module's policies, so it is built from this list; its
//! `assistant`, `connections` and `appearance` follow.

use crate::{Composition, config};
use app_module::Module;

/// What the server, `worker`, `migrate`, `seed` and `dump-spec` run.
pub struct Registered;

impl Composition for Registered {
    /// Nest sub-tuples past 12 entries, where std stops deriving `Default` for
    /// tuples (poem-openapi's own cap is 16).
    type Endpoints = (
        access::AccessEndpoints,
        assistant::AssistantEndpoints,
        connections::ConnectionsEndpoints,
        appearance::AppearanceEndpoints,
    );

    fn modules() -> Vec<Module> {
        vec![
            access::module(Self::modules),
            assistant::module(config::agent),
            connections::module(),
            appearance::module(),
        ]
    }
}
EOF
references=$(sed -nE 's/^name = "([^"]+)"/\1/p' crates/reference/*/core/Cargo.toml | paste -sd '|' -)
REFERENCES=$references perl -i -ne 'print unless /^($ENV{REFERENCES})\.workspace = true$/' crates/server/Cargo.toml
perl -i -pe 's{^seed = \[.*\]$}{seed = ["assistant/seed"]}' crates/server/Cargo.toml
perl -0 -i -pe 's{const TOOLS: &\[&str\] = &\[.*?\];}{const TOOLS: &[&str] = &[\n    "list_grants",\n    "grant_access",\n    "grant_access_in_bulk",\n    "revoke_grant",\n    "get_access_catalog",\n    "list_roles",\n    "list_subjects",\n    "create_role",\n    "delete_role",\n    "assign_role",\n    "unassign_role",\n    "invite_user",\n    "list_invitations",\n    "resend_invitation",\n    "revoke_invitation",\n    "list_connections",\n    "get_connection",\n    "update_connection",\n    "test_connection",\n    "get_theme",\n    "update_theme",\n];}s' crates/server/tests/composed_app.rs
# No seed assigns the dev sign-in (frontend/vite.config.ts) `admin` once the
# reference users are off the server: `make dev` makes it a bootstrap admin
# instead, unless `.env` sets APP_BOOTSTRAP_ADMINS, and `.env.example` says so.
dev_subject=00000001-0001-0000-0000-000000000001
perl -i -pe 's{^APP_BOOTSTRAP_ADMINS=.*$}{APP_BOOTSTRAP_ADMINS='"$dev_subject"'}' .env.example
perl -0 -i -pe 's{^(dev: )}{# No seed assigns the dev sign-in (frontend/vite.config.ts) `admin`.\n${1}export APP_BOOTSTRAP_ADMINS ?= '"$dev_subject"'\n$1}m' Makefile
perl -0 -i -pe 's{    \(\n        "register_user",.*?\),\n}{}s' crates/server/src/exempt.rs

# 5. The UI down to the shell (src/shell/: the sidebar and a launcher with no
# apps yet) and the platform's access and assistant screens.
git rm -rq frontend/src/features/notes frontend/src/features/note-search \
  frontend/src/pages/notes-page.tsx frontend/src/pages/search-page.tsx \
  frontend/src/test/fixtures.ts frontend/src/test/notes-list.test.tsx \
  frontend/src/test/note-editing.test.tsx frontend/src/test/note-search.test.tsx
cat >frontend/src/shell/capabilities.ts <<EOF
import { useMyPermissions } from "@/features/access";
import type { LinkProps } from "@tanstack/react-router";
import {
  KeyRoundIcon,
  ListChecksIcon,
  type LucideIcon,
  MailIcon,
  PaletteIcon,
  PlugIcon,
  ShieldIcon,
  UsersIcon,
} from "lucide-react";

/** Names the whole suite where no one app is open: the launcher's heading, the switcher. */
export const productName = "$title";

/**
 * One page of an app. It is shown only to a caller who holds \`needs\`, an action named as its
 * Module's policies name it (\`make check-modules\` fails on one no Module declares). Hiding is a
 * convenience: the server still refuses the operations themselves.
 */
export type Area = {
  label: string;
  to: NonNullable<LinkProps["to"]>;
  icon: LucideIcon;
  needs: { resource: string; action: string };
};

/**
 * One Capability of the suite (ADR 0047) and its app: every route under \`path\` is one of its
 * \`areas\` or a record in one. \`id\` is its \`capability.yaml\`'s, which \`make check-modules\` reads
 * from here as a literal \`id: "<id>"\`.
 */
export type Capability = {
  id: string;
  label: string;
  description: string;
  icon: LucideIcon;
  path: string;
  /** In the sidebar's order; the first is where the app opens. */
  areas: [Area, ...Area[]];
};

const access: Capability = {
  id: "access",
  label: "Access",
  description: "Who may use the apps, and what each role may do.",
  icon: ShieldIcon,
  path: "/access",
  areas: [
    {
      label: "Users",
      to: "/access/subjects",
      icon: UsersIcon,
      needs: { resource: "subject", action: "list" },
    },
    {
      label: "Roles",
      to: "/access/roles",
      icon: KeyRoundIcon,
      needs: { resource: "role", action: "list" },
    },
    {
      label: "Invitations",
      to: "/access/invitations",
      icon: MailIcon,
      needs: { resource: "invitation", action: "list" },
    },
    {
      label: "Permissions",
      to: "/access/permissions",
      icon: ListChecksIcon,
      needs: { resource: "grant", action: "list" },
    },
  ],
};

const integrations: Capability = {
  id: "connections",
  label: "Integrations",
  description: "The other systems the apps call: where each is, and its credentials.",
  icon: PlugIcon,
  path: "/integrations",
  areas: [
    {
      label: "Integrations",
      to: "/integrations",
      icon: PlugIcon,
      needs: { resource: "connection", action: "list" },
    },
  ],
};

const appearance: Capability = {
  id: "appearance",
  label: "Appearance",
  description: "The organisation's theme: its colours, fonts and corners.",
  icon: PaletteIcon,
  path: "/appearance",
  areas: [
    {
      label: "Appearance",
      to: "/appearance",
      icon: PaletteIcon,
      needs: { resource: "theme", action: "update" },
    },
  ],
};

/** Every app, and the platform's tools, kept apart in the sidebar's footer, in the switcher's order. */
export const registry: { apps: Capability[]; platform: Capability[] } = {
  apps: [],
  platform: [access, integrations, appearance],
};

/**
 * The apps the caller may open, each with only the areas whose \`needs\` they hold, leaving out an
 * app with none; nothing until the server has answered.
 */
export function useCapabilities(): { apps: Capability[]; platform: Capability[] } {
  const permissions = useMyPermissions() ?? [];
  const held = (area: Area) =>
    permissions.some((p) => p.resource === area.needs.resource && p.action === area.needs.action);
  const usable = (capabilities: Capability[]) =>
    capabilities.flatMap((capability) => {
      const [first, ...rest] = capability.areas.filter(held);
      return first ? [{ ...capability, areas: [first, ...rest] as Capability["areas"] }] : [];
    });
  return { apps: usable(registry.apps), platform: usable(registry.platform) };
}

/** The app whose \`path\` holds \`pathname\`, and its area: the one whose \`to\` holds it most closely. */
export function locate(pathname: string, apps: Capability[]): { app?: Capability; area?: Area } {
  const holds = (path: string) => pathname === path || pathname.startsWith(\`\${path}/\`);
  const app = apps.find((candidate) => holds(candidate.path));
  const area = app?.areas
    .filter((candidate) => holds(candidate.to))
    .sort((a, b) => b.to.length - a.to.length)[0];
  return { app, area };
}
EOF
cat >frontend/src/router.tsx <<'EOF'
import type { GrantSort } from "@/api/types.gen";
import { type ListSearch, validateListSearch } from "@/components/data-table";
import { type GrantFilter, grantSortKeys, invitationStatuses } from "@/features/access";
import {
  AccessInvitationsPage,
  AccessPermissionsPage,
  AccessRolesPage,
  AccessUsersPage,
} from "@/pages/access-page";
import { AppearancePage } from "@/pages/appearance-page";
import { AssistantPage } from "@/pages/assistant-page";
import { HomePage } from "@/pages/home-page";
import { IntegrationPage, IntegrationsPage } from "@/pages/integrations-page";
import { RolePage } from "@/pages/role-page";
import { RootLayout } from "@/pages/root-layout";
import { SignInCallbackPage } from "@/pages/sign-in-callback-page";
import { SignedOutPage } from "@/pages/signed-out-page";
import {
  type RouterHistory,
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
} from "@tanstack/react-router";

const rootRoute = createRootRoute();

// Pathless: every screen but signing in and out, which a visitor reaches before they are signed in.
const appRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "app",
  component: RootLayout,
});

const callbackRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/auth/callback",
  component: SignInCallbackPage,
});
const signedOutRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/auth/signed-out",
  component: SignedOutPage,
});

const indexRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/",
  component: HomePage,
});

// `?tab=` is where the access screen's areas were when they were tabs of one page.
const accessRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/access",
  beforeLoad: ({ search }) => {
    const { tab, ...rest } = search as Record<string, unknown>;
    throw redirect({
      to:
        tab === "roles"
          ? "/access/roles"
          : tab === "grants"
            ? "/access/permissions"
            : tab === "invitations"
              ? "/access/invitations"
              : "/access/subjects",
      search: rest,
    });
  },
});
const accessUsersRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/access/subjects",
  validateSearch: (search: Record<string, unknown>): { search?: string } => ({
    search: typeof search.search === "string" && search.search ? search.search : undefined,
  }),
  component: AccessUsersPage,
});
const accessRolesRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/access/roles",
  component: AccessRolesPage,
});
const accessInvitationsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/access/invitations",
  validateSearch: (
    search: Record<string, unknown>,
  ): { status?: (typeof invitationStatuses)[number] } => ({
    status: invitationStatuses.find((s) => s === search.status),
  }),
  component: AccessInvitationsPage,
});
const accessPermissionsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/access/permissions",
  validateSearch: (search: Record<string, unknown>): GrantFilter & ListSearch<GrantSort> => ({
    ...grantFilter(search),
    ...validateListSearch<GrantSort>(search, grantSortKeys),
  }),
  component: AccessPermissionsPage,
});

const roleRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/access/roles/$role",
  component: RolePage,
});

const integrationsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/integrations",
  component: IntegrationsPage,
});
const integrationRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/integrations/$key",
  component: IntegrationPage,
});

const appearanceRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/appearance",
  component: AppearancePage,
});

const assistantRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/assistant",
  validateSearch: (search: Record<string, unknown>): { conversation?: string } => ({
    conversation:
      typeof search.conversation === "string" && search.conversation
        ? search.conversation
        : undefined,
  }),
  component: AssistantPage,
});

const routeTree = rootRoute.addChildren([
  appRoute.addChildren([
    indexRoute,
    accessRoute,
    accessUsersRoute,
    accessRolesRoute,
    accessInvitationsRoute,
    accessPermissionsRoute,
    roleRoute,
    integrationsRoute,
    integrationRoute,
    appearanceRoute,
    assistantRoute,
  ]),
  callbackRoute,
  signedOutRoute,
]);

function grantFilter(search: Record<string, unknown>): GrantFilter {
  const text = (value: unknown) => (typeof value === "string" && value ? value : undefined);
  return { role: text(search.role), resource: text(search.resource), action: text(search.action) };
}

export function createAppRouter(history?: RouterHistory) {
  return createRouter({ routeTree, history });
}

export type AppRouter = ReturnType<typeof createAppRouter>;

declare module "@tanstack/react-router" {
  interface Register {
    router: AppRouter;
  }
}
EOF
cat >frontend/src/router.test.tsx <<EOF
import { screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { renderAt } from "./test/render-app";

// Not "No apps yet": the first \`make new-module\` gives the launcher an app.
test("shows the launcher at /", async () => {
  renderAt("/");

  expect(await screen.findByRole("heading", { name: "$title" })).toBeInTheDocument();
});
EOF
cat >frontend/screens/scenarios.ts <<EOF
import type { SignInProvider } from "@/lib/sign-in";
import type { RequestHandler } from "msw";
import type { Locator, Page } from "playwright";
import { assistantScenarios } from "./assistant-scenarios.ts";
import { integrationsScenarios } from "./integrations-scenarios.ts";
import { signInScenarios } from "./sign-in-scenarios.ts";

// Read by both the browser entry (main.tsx) and the Node script (shoot.ts), which
// runs it with Node's type stripping: so only package imports, types from \`@/\`, and other scenarios.

export type Scenario = {
  /** The screenshot's file name: \`<name>.<width>.png\`. */
  name: string;
  route: string;
  handlers: RequestHandler[];
  /** What the user does before the shot, found by role and label as the Vitest tests do. */
  steps?: (page: Page) => Promise<void>;
  /** What the shot must show; the run fails if it never appears. */
  shows: (page: Page) => Locator;
  /** The identity provider, when the app signs people in; with none it signs no one in, as in dev. */
  signIn?: () => SignInProvider;
};

export const scenarios: Scenario[] = [
  {
    name: "home",
    route: "/",
    handlers: [],
    shows: (page) => page.getByRole("heading", { name: "$title" }),
  },
  {
    name: "home-dark",
    route: "/",
    handlers: [],
    steps: async (page) => {
      await page.getByRole("button", { name: "Theme" }).click();
      await page.getByRole("menuitemradio", { name: "Dark" }).click();
      await page.keyboard.press("Escape");
    },
    shows: (page) => page.locator("html.dark").getByRole("heading", { name: "$title" }),
  },
  ...assistantScenarios,
  ...integrationsScenarios,
  ...signInScenarios,
];
EOF

# The UI's reference feature is gone from the tree: its docs link to it upstream.
UPSTREAM=$upstream perl -i -pe '
  s{`(src/(?:features/note[^`]*|pages/(?:notes|search)-page\.tsx|test/(?:notes-list|note-editing|note-search)\.test\.tsx|test/fixtures\.ts))`}{
    $ENV{UPSTREAM} ? "[`$1`]($ENV{UPSTREAM}/frontend/$1)" : "`$1` (`git show $ENV{COMMIT}:frontend/$1`)"
  }ge;
' frontend/AGENTS.md
perl -0 -i -pe '
  s{^\*\*Copy the reference code\*\* rather than working from prose: }{**Copy the reference code** rather than working from prose; until this app has a feature of its own to copy, that is the template'"'"'s, linked below at the commit in `TEMPLATE_VERSION`: }m;
  s{ `/home` and `/search` are old aliases of `/notes` and `/notes/search`, keeping their search\.}{}m;
  s{\(`/home`, `/search`, `/access\?tab=`\)}{(`/access?tab=`)}m;
' frontend/AGENTS.md

# The reference integration's fake goes with the reference modules: `make dev` runs it no more,
# and `make e2e` connects no integration until the app declares one of its own.
perl -0 -i -pe 's{^# --- reference integration: `make init` removes this block ---\n.*?^# --- end reference integration ---\n}{}ms' Makefile
perl -i -pe 's{^fake=".*"$}{fake=""}' scripts/e2e.sh

# 6. What made this; the commit leaves no way to run it again.
perl -0 -i -pe '
  s{^# --- template only: `make init` removes this block ---\n.*?^# --- end template only ---\n}{}ms;
  s{^(check: .*?) check-init( ##)}{$1$2}m;
' Makefile
perl -0 -i -pe 's{^  # --- template only: `make init` removes this job ---\n.*\z}{}ms' .github/workflows/ci.yml

# 7. Prove it, then commit.
make --no-print-directory spec generate
make --no-print-directory check
git rm -q scripts/init.sh scripts/check-init.sh
git add -A
git commit -q -m "Initialise $title from the template at $short"
trap - EXIT
echo "init: committed $(git rev-parse --short HEAD), \"Initialise $title from the template at $short\""
