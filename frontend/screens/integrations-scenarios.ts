import type { ConnectionResponse, ListResultConnectionResponse } from "@/api";
import { http, HttpResponse } from "msw";
import type { Scenario } from "./scenarios.ts";

// The platform's Integrations screens, which `make init` keeps. Like scenarios.ts, run by Node's
// type stripping: so only package imports, and types from `@/`.

const admin = http.get("*/api/access/me/permissions", () =>
  HttpResponse.json({ permissions: [{ resource: "connection", action: "list" }] }),
);

const crm: ConnectionResponse = {
  key: "crm",
  label: "CRM",
  auth: { style: "bearer", scopes: [] },
  url: "https://crm.example/api",
  fields: [{ name: "tenant", label: "Tenant", value: "acme" }],
  secrets: [
    { name: "token", label: "Token", set: true, setAt: "2026-11-26T09:00:00Z", setBy: "ada" },
  ],
  healthCheck: { method: "GET", path: "/health" },
  status: "ok",
  testedAt: "2026-11-26T09:05:00Z",
  version: "3",
};
const billing: ConnectionResponse = {
  ...crm,
  key: "billing",
  label: "Billing",
  url: "https://billing.example",
  status: "failing",
  lastError: "GET /health answered 401 Unauthorized: check the credentials",
};
const mail: ConnectionResponse = {
  ...crm,
  key: "mail",
  label: "Mail",
  url: undefined,
  fields: [],
  secrets: [{ name: "token", label: "Token", set: false }],
  status: "not_configured",
  testedAt: undefined,
};

const connections = http.get("*/api/connections", () =>
  HttpResponse.json<ListResultConnectionResponse>({ items: [billing, crm, mail], total: 3 }),
);
const one = http.get("*/api/connections/:key", ({ params }) =>
  HttpResponse.json([billing, crm, mail].find((c) => c.key === params.key)),
);

export const integrationsScenarios: Scenario[] = [
  {
    name: "integrations",
    route: "/integrations",
    handlers: [admin, connections],
    shows: (page) => page.getByRole("region", { name: "Integrations" }).getByText("Failing"),
  },
  {
    name: "integration-ok",
    route: "/integrations/crm",
    handlers: [admin, one],
    shows: (page) => page.getByRole("status", { name: "Test result" }).getByText("OK"),
  },
  {
    name: "integration-failing",
    route: "/integrations/billing",
    handlers: [admin, one],
    shows: (page) => page.getByText(/check the credentials/),
  },
  {
    name: "integration-not-configured",
    route: "/integrations/mail",
    handlers: [admin, one],
    shows: (page) => page.getByRole("button", { name: "Set Token" }),
  },
];
