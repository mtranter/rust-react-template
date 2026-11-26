import type {
  ConnectionResponse,
  ErrorBody,
  ErrorCode,
  ListResultConnectionResponse,
  SetSecretRequest,
  UpdateConnectionRequest,
} from "@/api";
import { http, HttpResponse } from "msw";

/** A Connection a module declares, with a bearer token and a health check, not yet filled in. */
export function connection(changes: Partial<ConnectionResponse> = {}): ConnectionResponse {
  return {
    key: "crm",
    label: "CRM",
    auth: { style: "bearer", scopes: [] },
    fields: [{ name: "tenant", label: "Tenant" }],
    secrets: [{ name: "token", label: "Token", set: false }],
    healthCheck: { method: "GET", path: "/health" },
    status: "not_configured",
    version: "1",
    ...changes,
  };
}

function refused(status: number, code: ErrorCode, message: string) {
  return HttpResponse.json<ErrorBody>({ code, message, requestId: "test" }, { status });
}

/**
 * `connections` as the server answers an admin: versioned with `If-Match`, secrets stored but
 * only ever answered as set, and Test connection answering `ok` once `secret` is the token set.
 * `values` holds every secret value the server was sent.
 */
export function connectionsApi(stored: ConnectionResponse[], working = "tok_live_working") {
  const connections = new Map(stored.map((c) => [c.key, { ...c }]));
  const values = new Map<string, string>();
  const bump = (c: ConnectionResponse) => {
    const configured = !!c.url && c.secrets.every((s) => s.set);
    c.version = String(Number(c.version) + 1);
    c.status = configured ? "untested" : "not_configured";
    c.lastError = undefined;
    c.testedAt = undefined;
  };
  const stale = (request: Request, c: ConnectionResponse) =>
    request.headers.get("If-Match") !== `"${c.version}"`;

  const handlers = [
    http.get("*/api/connections", () =>
      HttpResponse.json<ListResultConnectionResponse>({
        items: [...connections.values()],
        total: connections.size,
      }),
    ),
    http.get("*/api/connections/:key", ({ params }) => {
      const c = connections.get(String(params.key));
      return c ? HttpResponse.json(c) : refused(404, "not_found", "connection not found");
    }),
    http.patch("*/api/connections/:key", async ({ params, request }) => {
      const c = connections.get(String(params.key));
      if (!c) return refused(404, "not_found", "connection not found");
      if (stale(request, c)) return refused(412, "precondition_failed", "stale");
      const body = (await request.json()) as UpdateConnectionRequest;
      if (body.url !== undefined) c.url = body.url || undefined;
      for (const [name, value] of Object.entries(body.fields ?? {})) {
        const field = c.fields.find((f) => f.name === name);
        if (field) field.value = value || undefined;
      }
      bump(c);
      return HttpResponse.json(c);
    }),
    http.put("*/api/connections/:key/secrets/:name", async ({ params, request }) => {
      const c = connections.get(String(params.key));
      if (!c) return refused(404, "not_found", "connection not found");
      if (stale(request, c)) return refused(412, "precondition_failed", "stale");
      const secret = c.secrets.find((s) => s.name === params.name);
      if (!secret) return refused(404, "not_found", "secret not found");
      const { value } = (await request.json()) as SetSecretRequest;
      values.set(`${c.key}/${secret.name}`, value);
      Object.assign(secret, { set: true, setAt: "2026-11-26T09:00:00Z", setBy: "ada" });
      bump(c);
      return HttpResponse.json(c);
    }),
    http.post("*/api/connections/:key/test", ({ params }) => {
      const c = connections.get(String(params.key));
      if (!c) return refused(404, "not_found", "connection not found");
      if (c.status === "not_configured") {
        return refused(501, "not_configured", "connection_not_configured: give it a URL first");
      }
      const ok = [...c.secrets].every((s) => values.get(`${c.key}/${s.name}`) === working);
      c.status = ok ? "ok" : "failing";
      c.lastError = ok ? undefined : "GET /health answered 401 Unauthorized: check the credentials";
      c.testedAt = "2026-11-26T09:05:00Z";
      return HttpResponse.json(c);
    }),
  ];
  return { handlers, connections, values };
}
