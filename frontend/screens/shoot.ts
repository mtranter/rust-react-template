import { rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { createServer } from "vite";
import { scenarios } from "./scenarios.ts";

const widths = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 390, height: 844 },
};

const frontend = fileURLToPath(new URL("..", import.meta.url));
const out = fileURLToPath(new URL("out/", import.meta.url));

await rm(out, { recursive: true, force: true });

const server = await createServer({
  root: frontend,
  logLevel: "warn",
  server: { port: 0, hmr: false },
});
await server.listen();
const origin = server.resolvedUrls?.local[0];
if (!origin) throw new Error("vite didn't say where it listens");

const browser = await chromium.launch();
const failed: string[] = [];
try {
  for (const scenario of scenarios) {
    for (const [width, viewport] of Object.entries(widths)) {
      const shot = `${scenario.name}.${width}`;
      const context = await browser.newContext({ viewport, locale: "en-US", timezoneId: "UTC" });
      const page = await context.newPage();
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      // Chromium logs every 4xx and 5xx as an error, though a scenario's refusals are intended.
      page.on("console", (message) => {
        if (message.type() === "error" && !message.text().startsWith("Failed to load resource")) {
          errors.push(message.text());
        }
      });
      try {
        await page.goto(`${origin}screens/index.html?scenario=${scenario.name}`);
        await scenario.steps?.(page);
        await scenario.shows(page).waitFor({ timeout: 10_000 });
        if (errors.length > 0) throw new Error(errors.join("\n"));
        await page.screenshot({
          path: `${out}${shot}.png`,
          fullPage: true,
          animations: "disabled",
        });
        console.log(`screens/out/${shot}.png`);
      } catch (error) {
        failed.push(shot);
        console.error(`${shot}: ${error instanceof Error ? error.message : error}`);
      } finally {
        await context.close();
      }
    }
  }
} finally {
  await browser.close();
  await server.close();
}

if (failed.length > 0) {
  console.error(`failed: ${failed.join(", ")}`);
  process.exit(1);
}
