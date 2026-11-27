import assert from "node:assert/strict";
import { type ChildProcess, spawn } from "node:child_process";
import { once } from "node:events";
import { createServer } from "node:net";
import { createInterface } from "node:readline";
import { setTimeout as sleep } from "node:timers/promises";
import { chromium } from "playwright";

// `make e2e`: signs in to the binary's own SPA through `fake-oidc`, as a bootstrap admin, loads
// the access screen with that token, invites someone through `fake-oidc`'s stand-in for Cognito,
// connects an integration (a URL and a secret on Integrations, then Test connection, against the
// stateful fake $E2E_FAKE serves), saves a theme on Appearance that restyles the app, after a
// reload and for a visitor not signed in, then resets it, and signs out. `make e2e` builds the binaries (debug, the
// server with `embed-frontend`) into $E2E_BIN and migrates and seeds $E2E_DATABASE_URL. Without
// $E2E_FAKE (an app with no integration yet) the Integrations step only opens the list.

const bin = required("E2E_BIN");
const databaseUrl = required("E2E_DATABASE_URL");
const clientId = "e2e-spa";
const subject = { sub: "e2e-ada", name: "Ada Signed-In", email: "ada@e2e.test" };
/**
 * `<binary> <connection key> <secret name>`: a fake of another system that prints its URL first,
 * answering its health check only to `--token <secret>`, and the Connection to point at it.
 */
const fake = process.env.E2E_FAKE ? process.env.E2E_FAKE.split(" ") : undefined;
const fakeToken = "e2e-integration-token";
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
    // 32 bytes of 7, base64: what the server seals the integration's secret with.
    CONNECTIONS_MASTER_KEY: Buffer.alloc(32, 7).toString("base64"),
    COGNITO_USER_POOL_ID: "ap-southeast-2_e2e",
    COGNITO_ENDPOINT: `${issuer}/cognito`,
    AWS_ACCESS_KEY_ID: "e2e",
    AWS_SECRET_ACCESS_KEY: "e2e",
  });
  await ready(`${app}/health/ready`, server);

  const page = await (await browser.newContext({ locale: "en-US" })).newPage();
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const authorizations: string[] = [];
  const unsigned: string[] = [];
  // Public (`server::PUBLIC`): the SPA themes itself before sign-in (ADR 0051).
  const publicApi = [`${app}/api/appearance/theme`];
  page.on("request", (request) => {
    const url = request.url();
    if (url.startsWith(`${issuer}/authorize`)) authorizations.push(url);
    if (
      url.startsWith(`${app}/api/`) &&
      !publicApi.includes(url) &&
      !request.headers().authorization?.startsWith("Bearer ")
    ) {
      unsigned.push(url);
    }
  });

  // Signed out, a deep link leaves for the provider, which approves at once, and comes back to it.
  await page.goto(`${app}/access/subjects?search=${encodeURIComponent(subject.email)}`);
  const row = page.getByRole("row").filter({ hasText: subject.email });
  await row.getByText("Always admin").waitFor({ timeout: 30_000 });
  assert.equal(new URL(page.url()).pathname, "/access/subjects");
  assert.equal(new URL(page.url()).searchParams.get("search"), subject.email);
  assert.equal(authorizations.length, 1, "signed in once");
  // Only an admin's token lists the subjects, and the name shown came from this subject's token.
  await row.getByText(subject.name).waitFor();
  await row
    .getByRole("list", { name: `Roles of ${subject.name}` })
    .getByText("admin")
    .waitFor();
  await page.getByRole("banner").getByText(subject.name).waitFor();
  // A bootstrap admin holds every Area of Access by its built-in rules, with no grant of them.
  const areas = page.getByRole("navigation", { name: "Access" });
  for (const area of ["Users", "Roles", "Invitations", "Permissions"]) {
    await areas.getByRole("link", { name: area }).waitFor();
  }

  // Inviting goes through the server's Cognito adapter to the stand-in, and lists them pending.
  await areas.getByRole("link", { name: "Invitations" }).click();
  await page.getByRole("button", { name: "Invite user" }).click();
  const invite = page.getByRole("dialog", { name: "Invite user" });
  await invite.getByLabel("Email").fill("grace@e2e.test");
  await invite.getByLabel("Name").fill("Grace Invited");
  await invite.getByRole("button", { name: "Invite" }).click();
  const invited = page
    .getByRole("region", { name: "Invitations" })
    .getByRole("row")
    .filter({ hasText: "grace@e2e.test" });
  await invited.getByText("Pending").waitFor();
  await areas.getByRole("link", { name: "Users" }).click();
  await page.getByRole("row").filter({ hasText: "Grace Invited" }).waitFor();

  // Integrations: an admin gives a Connection its URL and secret, tests it and sees OK.
  await page.goto(`${app}/integrations`);
  const integrations = page.getByRole("region", { name: "Integrations" });
  await integrations.getByRole("table").waitFor();
  if (fake) {
    const [command, key, secret] = fake;
    const system = run("fake", command, ["--token", fakeToken]);
    const systemUrl = await system.firstLine();
    await page.goto(`${app}/integrations/${key}`);
    await page.getByLabel("URL").fill(systemUrl);
    await page.getByRole("button", { name: "Save" }).click();
    await page.getByText("Integration saved").waitFor();
    const input = page.locator(`#connection-secret-${secret}`);
    await input.fill(fakeToken);
    await page.getByRole("button", { name: /^Set / }).click();
    await page.getByRole("button", { name: /^Replace / }).waitFor();
    assert.equal(await input.inputValue(), "", "the secret field never shows a stored value");
    await page.getByRole("button", { name: "Test connection" }).click();
    await page
      .getByRole("status", { name: "Test result" })
      .getByText("OK", { exact: true })
      .waitFor();
    await page.goto(`${app}/integrations`);
    const row = integrations.getByRole("row").filter({ hasText: systemUrl });
    await row.getByText("OK", { exact: true }).waitFor();
    const read = await page.evaluate(async (key) => {
      const token = Object.entries(sessionStorage).find(([k]) => k.startsWith("oidc.user:"))?.[1];
      const access = token ? JSON.parse(token).access_token : "";
      const answer = await fetch(`/api/connections/${key}`, {
        headers: { Authorization: `Bearer ${access}` },
      });
      return answer.text();
    }, key);
    assert.ok(!read.includes(fakeToken), "the API never answers a secret's value");
  }
  assert.deepEqual(unsigned, [], "every API request carried the token");

  // Appearance: an admin's saved theme restyles the app, survives a reload and reaches a
  // visitor who has not signed in, with nothing rebuilt or restarted.
  await page.goto(`${app}/appearance`);
  const primary = page.getByLabel("Primary", { exact: true });
  await primary.waitFor();
  assert.notEqual(
    await page.getByLabel("Text", { exact: true }).getAttribute("placeholder"),
    "",
    "every colour shows its default",
  );
  await primary.fill("rgb(1, 2, 3)");
  const rootVar = (name: string) =>
    page.evaluate(
      (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim(),
      name,
    );
  assert.equal(await rootVar("--primary"), "rgb(1, 2, 3)", "an edit previews at once");
  await page.getByRole("button", { name: "Save" }).click();
  await page.getByText("Appearance saved").waitFor();
  await page.reload();
  await primary.waitFor();
  assert.equal(await rootVar("--primary"), "rgb(1, 2, 3)", "the saved theme survives a reload");
  const theme = await (await fetch(`${app}/api/appearance/theme`)).json();
  assert.equal(theme.light.primary, "rgb(1, 2, 3)", "anyone may read the theme");
  const visitor = await (await browser.newContext()).newPage();
  await visitor.goto(`${app}/auth/signed-out`);
  await visitor.waitForFunction(
    () =>
      getComputedStyle(document.documentElement).getPropertyValue("--primary").trim() ===
      "rgb(1, 2, 3)",
  );
  await visitor.context().close();
  await page.getByRole("button", { name: "Reset to defaults" }).click();
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await page.getByText("Appearance reset to the defaults").waitFor();
  assert.notEqual(await rootVar("--primary"), "rgb(1, 2, 3)", "a reset restores the defaults");
  await page.getByRole("button", { name: "Reset", exact: true }).waitFor({ state: "hidden" });

  await page.getByRole("button", { name: `Account: ${subject.name}` }).click();
  await page.getByRole("menuitem", { name: "Sign out" }).click();
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
  console.log(
    `e2e: signed in, loaded /access with the token, invited someone, ${fake ? "connected an integration and tested it OK, " : ""}saved a theme that restyled the app for a visitor too and reset it, signed out and stayed signed out`,
  );
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
        !entry[0].startsWith("COGNITO_") &&
        !entry[0].startsWith("CONNECTIONS_") &&
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
