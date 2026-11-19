import assert from "node:assert/strict";
import { type ChildProcess, spawn } from "node:child_process";
import { once } from "node:events";
import { createServer } from "node:net";
import { createInterface } from "node:readline";
import { setTimeout as sleep } from "node:timers/promises";
import { chromium } from "playwright";

// `make e2e`: signs in to the binary's own SPA through `fake-oidc`, as a bootstrap admin, loads
// the access screen with that token, and signs out. `make e2e` builds both binaries (debug, the
// server with `embed-frontend`) into $E2E_BIN and migrates and seeds $E2E_DATABASE_URL.

const bin = required("E2E_BIN");
const databaseUrl = required("E2E_DATABASE_URL");
const clientId = "e2e-spa";
const subject = { sub: "e2e-ada", name: "Ada Signed-In", email: "ada@e2e.test" };
/** How long the signed-out page must stay put to count as staying. */
const stays = 3_000;

const processes: Running[] = [];
const browser = await chromium.launch();
let passed = false;
try {
  const provider = run("fake-oidc", `${bin}/fake-oidc`, [
    ...["--client-id", clientId, "--subject", subject.sub],
    ...["--name", subject.name, "--email", subject.email],
  ]);
  const issuer = await provider.firstLine();

  const port = await freePort();
  const app = `http://127.0.0.1:${port}`;
  const server = run("server", `${bin}/server`, [], {
    DATABASE_URL: databaseUrl,
    APP_ENV: "prod",
    APP_HOST: "127.0.0.1",
    APP_PORT: String(port),
    APP_JWT_ISSUER: issuer,
    APP_OIDC_CLIENT_ID: clientId,
    APP_BOOTSTRAP_ADMINS: subject.sub,
  });
  await ready(`${app}/health/ready`, server);

  const page = await (await browser.newContext({ locale: "en-US" })).newPage();
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const authorizations: string[] = [];
  const unsigned: string[] = [];
  page.on("request", (request) => {
    const url = request.url();
    if (url.startsWith(`${issuer}/authorize`)) authorizations.push(url);
    if (url.startsWith(`${app}/api/`) && !request.headers().authorization?.startsWith("Bearer ")) {
      unsigned.push(url);
    }
  });

  // Signed out, a deep link leaves for the provider, which approves at once, and comes back to it.
  await page.goto(`${app}/access?tab=subjects`);
  const row = page.getByRole("row").filter({ hasText: subject.email });
  await row.getByText("Bootstrap admin").waitFor({ timeout: 30_000 });
  assert.equal(new URL(page.url()).pathname, "/access");
  assert.equal(new URL(page.url()).searchParams.get("tab"), "subjects");
  assert.equal(authorizations.length, 1, "signed in once");
  // Only an admin's token lists the subjects, and the name shown came from this subject's token.
  await row.getByText(subject.name).waitFor();
  await row
    .getByRole("list", { name: `Roles of ${subject.name}` })
    .getByText("admin")
    .waitFor();
  await page.getByRole("banner").getByText(subject.name).waitFor();
  assert.deepEqual(unsigned, [], "every API request carried the token");

  await page.getByRole("button", { name: "Sign out" }).click();
  const signedOut = page.getByRole("heading", { name: "You're signed out" });
  await signedOut.waitFor();
  await sleep(stays);
  assert.equal(new URL(page.url()).pathname, "/auth/signed-out");
  assert.ok(await signedOut.isVisible(), "the signed-out page stays");
  assert.equal(authorizations.length, 1, "signing out doesn't sign back in");
  const kept = await page.evaluate(() => Object.keys(sessionStorage));
  assert.deepEqual(
    kept.filter((key) => key.startsWith("oidc.user:")),
    [],
    "signing out forgets the tokens",
  );
  assert.deepEqual(errors, []);
  passed = true;
  console.log("e2e: signed in, loaded /access with the token, signed out and stayed signed out");
} finally {
  await browser.close();
  for (const running of processes) running.stop();
  if (!passed) for (const running of processes) running.dump();
}

type Running = {
  firstLine: () => Promise<string>;
  exited: Promise<number | null>;
  stop: () => void;
  dump: () => void;
};

/** Starts `command`, keeping its output to show if the check fails. */
function run(
  name: string,
  command: string,
  args: string[],
  env: Record<string, string> = {},
): Running {
  const child: ChildProcess = spawn(command, args, {
    env: { ...inherited(), ...env },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const output: string[] = [];
  child.stderr?.on("data", (chunk) => output.push(String(chunk)));
  const lines = createInterface({ input: child.stdout as NodeJS.ReadableStream });
  const first = new Promise<string>((resolve) => lines.once("line", resolve));
  lines.on("line", (line) => output.push(`${line}\n`));
  const exited = once(child, "exit").then(([code]) => code as number | null);
  const running: Running = {
    firstLine: () =>
      Promise.race([
        first,
        exited.then((code) => {
          throw new Error(`${name} exited (${code}) before it started:\n${output.join("")}`);
        }),
      ]),
    exited,
    stop: () => child.kill(),
    dump: () => console.error(`--- ${name}'s output ---\n${output.join("")}`),
  };
  processes.push(running);
  return running;
}

/**
 * This process's environment without the app's settings, which `make` exports from a developer's
 * `.env` and would change what the check signs in with.
 */
function inherited(): Record<string, string> {
  return Object.fromEntries(
    Object.entries(process.env).filter(
      (entry): entry is [string, string] =>
        entry[1] !== undefined &&
        !(entry[0].startsWith("APP_") && !entry[0].startsWith("APP_REDIS_")) &&
        entry[0] !== "DATABASE_URL",
    ),
  );
}

async function ready(url: string, server: Running) {
  let exited: number | null | undefined;
  void server.exited.then((code) => {
    exited = code;
  });
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    if (exited !== undefined) throw new Error(`the server exited (${exited}) before it was ready`);
    const answer = await fetch(url).catch(() => undefined);
    if (answer?.ok) return;
    await sleep(200);
  }
  throw new Error(`${url} wasn't ready within a minute`);
}

async function freePort(): Promise<number> {
  const probe = createServer();
  probe.listen(0, "127.0.0.1");
  await once(probe, "listening");
  const address = probe.address();
  probe.close();
  assert.ok(address && typeof address === "object");
  return address.port;
}

function required(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is unset: run this through \`make e2e\``);
  return value;
}
