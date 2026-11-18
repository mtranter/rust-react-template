#!/usr/bin/env bash
# Turns a fresh clone of the template into a greenfield app, once, in one
# commit: its own name and domain docs, the reference modules registered with
# no server, the UI down to a shell. The platform's modules (`crates/platform/`,
# `access`) stay registered: they are the template's, not reference code. The reference code stays, compiled and
# tested, for agents to copy until the app's ticket 0001 retires it. Then it
# removes itself, its make targets and its CI job.
#
#   scripts/init.sh <name> "<title>"    e.g. `crm "Acme CRM"` (`make init NAME=.. TITLE=..`)
#
# Refuses on a dirty tree, or once TEMPLATE_VERSION exists. Ends with `make
# check`; if that fails, the changes are left uncommitted to look at.
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
- [ ] The platform journeys in \`crates/reference/journeys/\` (\`agents\`, \`compacting_history\`) are rewritten over the app's modules in \`crates/server/tests/journeys/\`, so the platform keeps its coverage.
- [ ] \`crates/platform/\` stays: \`access\` is the platform's, registered in every app, not reference code.
- [ ] \`crates/reference/\`, its workspace members and dependencies in the root \`Cargo.toml\`, its packages in \`deny.toml\`, its queries in \`.sqlx\` (\`make prepare\`) and its mentions in \`scripts/\` are gone, and \`make check\` passes.
EOF

# 3. The app's name.
perl -i -pe 's{^pub const API_TITLE: &str = ".*";}{pub const API_TITLE: &str = "$ENV{TITLE} API";}' crates/server/src/lib.rs
perl -i -pe 's{<title>.*</title>}{<title>$ENV{TITLE}</title>}' frontend/index.html
perl -i -pe 's{^  "name": ".*",}{  "name": "$ENV{NAME}",}' frontend/package.json
perl -0 -i -pe '
  s{\A# AGENTS\.md\n\n.*?\n\n}{# AGENTS.md\n\n**$ENV{TITLE}**: a Rust modular-monolith backend (Poem + poem-openapi) whose OpenAPI spec drives a type-safe Vite + React + TypeScript frontend. Started from a template, at the commit `TEMPLATE_VERSION` records; the platform it brought is described in `docs/platform/`.\n\n}s;
  s{^- `make init .*\n}{}m;
' AGENTS.md

# 4. The reference modules off the server: compiled and tested, registered nowhere.
# The platform's `access` stays, granting from whatever the app registers.
cat >crates/server/src/modules.rs <<'EOF'
//! The one place a module is registered: its `Endpoints` in the tuple and its
//! `module()` in the list (`make new-module` adds both; `make check-modules`
//! fails on a module missing from either). The platform's `access` comes first:
//! it grants from every module's policies, so it is built from this list.

use crate::Composition;
use app_module::Module;

/// What the server, `worker`, `migrate`, `seed` and `dump-spec` run.
pub struct Registered;

impl Composition for Registered {
    /// Nest sub-tuples past 12 entries, where std stops deriving `Default` for
    /// tuples (poem-openapi's own cap is 16).
    type Endpoints = access::AccessEndpoints;

    fn modules() -> Vec<Module> {
        vec![access::module(Self::modules)]
    }
}
EOF
references=$(sed -nE 's/^name = "([^"]+)"/\1/p' crates/reference/*/core/Cargo.toml | paste -sd '|' -)
REFERENCES=$references perl -i -ne 'print unless /^($ENV{REFERENCES})\.workspace = true$/' crates/server/Cargo.toml
perl -i -pe 's{^seed = \[.*\]$}{seed = []}' crates/server/Cargo.toml
perl -0 -i -pe 's{const TOOLS: &\[&str\] = &\[.*?\];}{const TOOLS: &[&str] = &[\n    "list_grants",\n    "grant_access",\n    "revoke_grant",\n    "get_access_catalog",\n];}s' crates/server/tests/composed_app.rs
perl -0 -i -pe 's{pub const PUBLIC: &\[\(&str, &str\)\] = &\[.*?\];}{pub const PUBLIC: &[(&str, &str)] = &[];}s' crates/server/src/public.rs

# 5. The UI down to a shell: the root layout, a placeholder home page, and the
# platform's access screen.
git rm -rq frontend/src/features/notes frontend/src/features/note-search \
  frontend/src/pages/search-page.tsx frontend/src/test/fixtures.ts \
  frontend/src/test/notes-list.test.tsx frontend/src/test/note-editing.test.tsx \
  frontend/src/test/note-search.test.tsx
cat >frontend/src/pages/home-page.tsx <<EOF
export function HomePage() {
  return (
    <main className="mx-auto max-w-4xl space-y-6 p-6">
      <h1 className="font-heading text-2xl font-semibold">$title</h1>
      <p className="text-muted-foreground">Nothing here yet.</p>
    </main>
  );
}
EOF
cat >frontend/src/router.tsx <<'EOF'
import type { GrantSort } from "@/api/types.gen";
import { type ListSearch, validateListSearch } from "@/components/data-table";
import { type GrantFilter, grantSortKeys } from "@/features/access";
import { AccessPage } from "@/pages/access-page";
import { HomePage } from "@/pages/home-page";
import { RootLayout } from "@/pages/root-layout";
import {
  type RouterHistory,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";

const rootRoute = createRootRoute({ component: RootLayout });

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: HomePage,
});

const accessRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/access",
  validateSearch: (search: Record<string, unknown>): GrantFilter & ListSearch<GrantSort> => ({
    ...grantFilter(search),
    ...validateListSearch<GrantSort>(search, grantSortKeys),
  }),
  component: AccessPage,
});

const routeTree = rootRoute.addChildren([indexRoute, accessRoute]);

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

test("shows the home page at /", async () => {
  renderAt("/");

  expect(await screen.findByRole("heading", { name: "$title" })).toBeInTheDocument();
});
EOF
perl -0 -i -pe '
  s{^import \{ \w+ \} from "\@/test/fixtures";\n}{}m;
  s{^import \{ server \} from "\.\./\.\./vitest\.setup";\n}{}m;
  s{^  server\.use\(.*?\);\n}{}m;
' frontend/src/test/theme.test.tsx
cat >frontend/screens/scenarios.ts <<EOF
import type { RequestHandler } from "msw";
import type { Locator, Page } from "playwright";

// Read by both the browser entry (main.tsx) and the Node script (shoot.ts), which
// runs it with Node's type stripping: so only package imports, and types from \`@/\`.

export type Scenario = {
  /** The screenshot's file name: \`<name>.<width>.png\`. */
  name: string;
  route: string;
  handlers: RequestHandler[];
  /** What the user does before the shot, found by role and label as the Vitest tests do. */
  steps?: (page: Page) => Promise<void>;
  /** What the shot must show; the run fails if it never appears. */
  shows: (page: Page) => Locator;
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
];
EOF

# The UI's reference feature is gone from the tree: its docs link to it upstream.
UPSTREAM=$upstream perl -i -pe '
  s{`(src/(?:features/note[^`]*|pages/(?:home|search)-page\.tsx|test/(?:notes-list|note-editing|note-search)\.test\.tsx|test/fixtures\.ts))`}{
    $ENV{UPSTREAM} ? "[`$1`]($ENV{UPSTREAM}/frontend/$1)" : "`$1` (`git show $ENV{COMMIT}:frontend/$1`)"
  }ge;
' frontend/AGENTS.md
perl -0 -i -pe '
  s{^\*\*Copy the reference code\*\* rather than working from prose: }{**Copy the reference code** rather than working from prose; until this app has a feature of its own to copy, that is the template'"'"'s, linked below at the commit in `TEMPLATE_VERSION`: }m;
  s{ `/home` is an old alias: it redirects to `/`, keeping its search\.}{}m;
' frontend/AGENTS.md

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
echo "init: committed $(git rev-parse --short HEAD), \"Initialise $title from the template at $short\""
