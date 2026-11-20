import { http, HttpResponse } from "msw";
import type { Scenario } from "./scenarios.ts";

// The platform's assistant screens, which `make init` keeps. Like scenarios.ts, run by Node's type
// stripping: so only package imports, and types from `@/`.

const assistantOn = http.get("*/api/assistant/status", () => HttpResponse.json({ on: true }));

export const assistantScenarios: Scenario[] = [
  {
    name: "assistant",
    route: "/assistant",
    handlers: [assistantOn],
    shows: (page) => page.getByRole("region", { name: "Chat" }),
  },
];
