#!/usr/bin/env bash
# Runs `make init` on a scratch copy of this tree (uncommitted changes
# included) as a factory would on a fresh clone, so a platform change can't
# silently break it. `init` ends in the app's own `make check`; this then checks
# what that can't: that `init` refuses what it should, puts the tree back when a
# step fails, leaves no trace of the
# reference code on the app's surface, and leaves an app `make new-module` can
# grow.
#
# The copy is target/init-check/app, built into target/init-check/target, over
# the docker compose Postgres this checkout uses, in a database of its own.
set -euo pipefail
cd "$(dirname "$0")/.."

root=$PWD
work=$root/target/init-check
app=$work/app
name=crm
title="Acme CRM"

fail() {
  echo "check-init: $1" >&2
  exit 1
}

make --no-print-directory db-wait
COMPOSE_PROJECT_NAME=$(docker compose config --format json | jq -r .name)
export COMPOSE_PROJECT_NAME
export CARGO_TARGET_DIR=$work/target
export GIT_AUTHOR_NAME=check-init GIT_AUTHOR_EMAIL=check-init@localhost
export GIT_COMMITTER_NAME=check-init GIT_COMMITTER_EMAIL=check-init@localhost
# `make migrate` and `make new-module` there change this database, never the dev one.
docker compose exec -T postgres psql -q -U app -d postgres \
  -c 'drop database if exists init_check with (force)' -c 'create database init_check'
export DATABASE_URL=postgres://app:app@localhost:5432/init_check

rm -rf "$app"
mkdir -p "$app"
git ls-files -z -co --exclude-standard | while IFS= read -r -d '' path; do
  [ -e "$path" ] && printf '%s\0' "$path"
done | tar -cf - --null -T - | tar -xf - -C "$app"
[ -d .bin ] && ln -s "$root/.bin" "$app/.bin"
cd "$app"
git init -q -b main
git remote add origin "$(git -C "$root" remote get-url origin 2>/dev/null || echo "$root")"
git add -A
git commit -q -m "The template"
template=$(git rev-parse HEAD)

unchanged() {
  [ -z "$(git status --porcelain)" ] && [ "$(git rev-parse HEAD)" = "$template" ] ||
    fail "a refused \`make init\` $1 changed the tree"
}
! make --no-print-directory init >/dev/null 2>&1 || fail "\`make init\` ran without NAME and TITLE"
unchanged "without NAME and TITLE"
! make --no-print-directory init NAME=Not_A_Name TITLE="$title" >/dev/null 2>&1 || fail "\`make init\` took NAME=Not_A_Name"
unchanged "with a bad NAME"
touch untracked
! make --no-print-directory init NAME=$name TITLE="$title" >/dev/null 2>&1 || fail "\`make init\` ran on a dirty tree"
rm untracked
unchanged "on a dirty tree"
# A `make` that fails once `init` has edited the tree, as a failing `make check` would.
mkdir -p "$work/failing"
printf '#!/bin/sh\ncase " $* " in *" spec "*) exit 1 ;; esac\nexec "%s" "$@"\n' "$(command -v make)" >"$work/failing/make"
chmod +x "$work/failing/make"
! PATH="$work/failing:$PATH" make --no-print-directory init NAME=$name TITLE="$title" >/dev/null 2>&1 ||
  fail "\`make init\` passed though a step failed"
unchanged "by a failing step"
git stash list | grep -q "make init, failed" || fail "a failed \`make init\` kept nothing to look at"
git stash drop -q

make --no-print-directory init NAME=$name TITLE="$title"

[ "$(git rev-list --count "$template..HEAD")" = 1 ] || fail "\`make init\` didn't end in one commit"
[ -z "$(git status --porcelain)" ] || fail "\`make init\` left changes uncommitted: $(git status --porcelain)"
! make --no-print-directory init NAME=$name TITLE="$title" >/dev/null 2>&1 || fail "\`make init\` ran twice"
for made in TEMPLATE_VERSION GLOSSARY.md docs/adr/.gitkeep docs/tickets/0001-retire-the-reference-modules.md; do
  [ -f "$made" ] || fail "\`make init\` didn't make $made"
done
grep -qx "commit $template" TEMPLATE_VERSION || fail "TEMPLATE_VERSION doesn't name $template"
git diff --quiet "$template" HEAD -- docs/platform || fail "\`make init\` changed docs/platform/"
# The platform's `access`, `assistant`, `connections` and `appearance` stay registered: their
# operations are the only ones, and theirs the only MCP tools (the assistant's,
# and connections' on secrets, are REST-only).
# composed_app.rs, which `init`'s `make check` ran, fails on a tool not in its TOOLS.
access_ops='["assign_role","create_role","delete_role","get_access_catalog","grant_access","grant_access_in_bulk","invite_user","list_grants","list_invitations","list_roles","list_subjects","resend_invitation","revoke_grant","revoke_invitation","unassign_role"]'
platform_tools=$(jq -c '. + ["list_connections","get_connection","update_connection","test_connection","get_theme","update_theme"] | sort' <<<"$access_ops")
platform_ops=$(jq -c '. + ["set_connection_secret","clear_connection_secret","decide_tool_call","get_my_permissions","delete_conversation","get_assistant_status","get_conversation","list_conversations","send_message","start_conversation"] | sort' <<<"$platform_tools")
jq -e --argjson ops "$platform_ops" '[.paths[][].operationId] | sort == $ops' frontend/openapi.json >/dev/null ||
  fail "the app's spec has operations other than the platform's: $(jq -c '[.paths[][].operationId]' frontend/openapi.json)"
[ "$(sed -n '/^const TOOLS/,/^];/p' crates/server/tests/composed_app.rs | grep -oE '"[a-z_]+"' | tr -d '"' | sort | paste -sd, -)" = \
  "$(jq -r --argjson ops "$platform_tools" '$ops | join(",")' <<<null)" ] ||
  fail "the app's MCP tools aren't only the platform's"
grep -q "<title>$title</title>" frontend/index.html || fail "frontend/index.html isn't titled $title"
! git grep -nE 'make init NAME|check-init|scripts/init\.sh|^init:' -- AGENTS.md Makefile .github scripts ||
  fail "the app still offers \`make init\`, which it no longer has"

# The reference code stays, for agents to copy, but nothing else names it: what
# does is on the app's surface. Lines naming crates/reference/, or the template
# commit the docs link to, point at it as reference. The rest are the reference
# crates' build records: their queries, their lockfile entries, and their
# entries in deny.toml's lists, which name every module crate's package.
named=$(git grep -nIwE 'rust-vite-template|notes|users|note-search|note_search|notes_api|users_api' -- . \
  ':!crates/reference/' ':!docs/platform/' ':!TEMPLATE_VERSION' ':!.sqlx/' ':!Cargo.lock' ':!deny.toml' |
  grep -vF -e crates/reference/ -e "$template" || true)
[ -z "$named" ] || fail "the app names the template or its reference code:
$named"

# An app starts with no Capabilities, so with no workspace members for them:
# a glob matching nothing would fail the workspace.
[ ! -e crates/capabilities ] || fail "\`make init\` left crates/capabilities/"
! grep -v '^ *#' Cargo.toml | grep -qF 'crates/capabilities/' || fail "\`make init\` left Cargo.toml naming crates/capabilities/"

make --no-print-directory new-module CAPABILITY=crm NAME=contacts
for made in crates/capabilities/crm/capability.yaml crates/capabilities/crm/contacts/core crates/capabilities/crm/contacts/api; do
  [ -e "$made" ] || fail "\`make new-module CAPABILITY=crm NAME=contacts\` didn't make $made"
done
grep -qF 'id: "crm",' frontend/src/shell/capabilities.ts ||
  fail "\`make new-module\` didn't give the new \`crm\` Capability an entry in the SPA's registry"
make --no-print-directory check-modules
SQLX_OFFLINE=true DATABASE_URL=postgres://app:app@localhost:5432/postgres \
  cargo bin cargo-nextest run -p contacts -p contacts-api -p server
# The new app's entry routes to a page, so the SPA still type-checks, lints and passes its tests.
make --no-print-directory generate typecheck lint-frontend test-frontend
echo "check-init ok"
