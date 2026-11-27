import type { Theme } from "@/api";
import { http, HttpResponse } from "msw";
import type { Scenario } from "./scenarios.ts";

// The platform's Appearance editor. Like scenarios.ts, run by Node's type stripping: so only
// package imports, and types from `@/`.

const admin = http.get("*/api/access/me/permissions", () =>
  HttpResponse.json({ permissions: [{ resource: "theme", action: "update" }] }),
);

const custom: Theme = {
  version: 3,
  light: {
    primary: "oklch(0.55 0.2 160)",
    ring: "oklch(0.6 0.18 160)",
    "sidebar-primary": "oklch(0.55 0.2 160)",
    highlight: "oklch(0.6 0.2 40)",
  },
  dark: { primary: "oklch(0.75 0.16 160)" },
  shared: { radius: "1rem", "font-sans": "ui-serif, Charter, 'Bitstream Charter', Georgia, serif" },
  updatedAt: "2026-11-27T09:00:00Z",
  updatedBy: "ada",
};

export const appearanceScenarios: Scenario[] = [
  {
    name: "appearance",
    route: "/appearance",
    handlers: [admin],
    shows: (page) => page.getByRole("region", { name: "Preview" }),
  },
  {
    name: "appearance-custom",
    route: "/appearance",
    handlers: [admin, http.get("*/api/appearance/theme", () => HttpResponse.json(custom))],
    steps: async (page) => {
      await page.getByRole("switch", { name: "Preview dark" }).click();
    },
    shows: (page) => page.getByLabel("Primary", { exact: true }),
  },
];
