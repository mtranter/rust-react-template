import type {
  CatalogResponse,
  ErrorBody,
  GrantRequest,
  GrantResponse,
  ListResultGrantResponse,
  ListResultRoleResponse,
  RoleRequest,
  RoleResponse,
} from "@/api";
import { http, HttpResponse } from "msw";

export const catalog: CatalogResponse = {
  resources: [
    {
      module: "workshop",
      resource: "module:workshop",
      actions: [{ name: "use", operations: [], reachable: true }],
      scopes: ["any"],
    },
    {
      module: "workshop",
      resource: "gadget",
      actions: [
        { name: "make", operations: ["make_gadget"], reachable: true },
        { name: "view", operations: ["get_gadget"], reachable: true },
        { name: "scrap", operations: [], reachable: false },
      ],
      scopes: ["any", "maker"],
    },
  ],
};

export function grant(n: number, changes: Partial<GrantResponse> = {}): GrantResponse {
  return {
    id: `grant-${n}`,
    role: "user",
    resource: "gadget",
    action: "make",
    scope: "any",
    grantedAt: new Date(Date.UTC(2026, 0, n)).toISOString(),
    stale: false,
    version: "1",
    ...changes,
  };
}

export function role(name: string, changes: Partial<RoleResponse> = {}): RoleResponse {
  const builtIn = name === "admin" || name === "user";
  return {
    name,
    description: "",
    builtIn,
    ...(builtIn ? {} : { createdBy: "an-admin" }),
    createdAt: new Date(Date.UTC(2026, 0, 1)).toISOString(),
    version: "1",
    ...changes,
  };
}

/** The built-in roles, and `support`, which an admin created. */
export function someRoles(): RoleResponse[] {
  return [role("admin"), role("support", { description: "Answers customers" }), role("user")];
}

function refusal(code: ErrorBody["code"], message: string, path?: string): ErrorBody {
  return { code, message, ...(path && { fields: [{ path, message }] }), requestId: "test" };
}

const forbidden = () => HttpResponse.json(refusal("forbidden", "forbidden"), { status: 403 });

/**
 * `access` as the server answers an admin for `items` and `roles` (changed in place): the
 * catalog, the grants filtered by role, resource and action, a grant checked against the roles
 * and the catalog and refused when equal to one made, revocations guarded by `If-Match`, and the
 * roles, created unless the name is taken and deleted unless built in or granted. `requests`
 * records each grant list's query and `ifMatches` each revocation's and deletion's `If-Match`.
 */
export function accessApi(items: GrantResponse[], roles: RoleResponse[] = someRoles()) {
  const requests: URLSearchParams[] = [];
  const ifMatches: (string | null)[] = [];
  const handlers = [
    http.get("*/api/access/catalog", () => HttpResponse.json(catalog)),
    http.get("*/api/access/grants", ({ request }) => {
      const params = new URL(request.url).searchParams;
      requests.push(params);
      const matching = items.filter((g) =>
        (["role", "resource", "action"] as const).every(
          (field) => !params.get(field) || g[field] === params.get(field),
        ),
      );
      return HttpResponse.json({
        items: matching,
        total: matching.length,
        limit: 20,
        offset: 0,
      } satisfies ListResultGrantResponse);
    }),
    http.post("*/api/access/grants", async ({ request }) => {
      const body = (await request.json()) as GrantRequest;
      if (!roles.some((r) => r.name === body.role)) {
        return HttpResponse.json(refusal("validation_error", "no such role", "role"), {
          status: 400,
        });
      }
      const resource = catalog.resources.find((r) => r.resource === body.resource);
      if (!resource) {
        return HttpResponse.json(refusal("validation_error", "no such resource", "resource"), {
          status: 400,
        });
      }
      if (!resource.actions.some((a) => a.name === body.action)) {
        return HttpResponse.json(refusal("validation_error", "no such action", "action"), {
          status: 400,
        });
      }
      const scope = body.scope ?? "any";
      if (
        items.some(
          (g) =>
            g.role === body.role &&
            g.resource === body.resource &&
            g.action === body.action &&
            g.scope === scope,
        )
      ) {
        return HttpResponse.json(refusal("conflict", "granted already"), { status: 409 });
      }
      const made = grant(items.length + 100, { ...body, scope, grantedBy: "an-admin" });
      items.push(made);
      return HttpResponse.json(made, { headers: { ETag: `"${made.version}"` } });
    }),
    http.delete("*/api/access/grants/:id", ({ params, request }) => {
      const index = items.findIndex((g) => g.id === params.id);
      if (index < 0) {
        return HttpResponse.json(refusal("not_found", "grant not found"), { status: 404 });
      }
      const ifMatch = request.headers.get("If-Match");
      ifMatches.push(ifMatch);
      if (ifMatch !== `"${items[index].version}"`) {
        return HttpResponse.json(refusal("precondition_failed", "the grant has changed"), {
          status: 412,
        });
      }
      items.splice(index, 1);
      return new HttpResponse(null, { status: 204 });
    }),
    http.get("*/api/access/roles", () =>
      HttpResponse.json({
        items: [...roles].sort((a, b) => a.name.localeCompare(b.name)),
        total: roles.length,
        limit: 100,
        offset: 0,
      } satisfies ListResultRoleResponse),
    ),
    http.post("*/api/access/roles", async ({ request }) => {
      const body = (await request.json()) as RoleRequest;
      if (roles.some((r) => r.name === body.name)) {
        return HttpResponse.json(refusal("conflict", "a role of that name exists already"), {
          status: 409,
        });
      }
      const made = role(body.name, { description: body.description ?? "" });
      roles.push(made);
      return HttpResponse.json(made, { headers: { ETag: `"${made.version}"` } });
    }),
    http.delete("*/api/access/roles/:name", ({ params, request }) => {
      const index = roles.findIndex((r) => r.name === params.name);
      if (index < 0) {
        return HttpResponse.json(refusal("not_found", "role not found"), { status: 404 });
      }
      ifMatches.push(request.headers.get("If-Match"));
      if (request.headers.get("If-Match") !== `"${roles[index].version}"`) {
        return HttpResponse.json(refusal("precondition_failed", "the role has changed"), {
          status: 412,
        });
      }
      if (roles[index].builtIn || items.some((g) => g.role === params.name)) {
        return HttpResponse.json(
          refusal("conflict", "a grant still names that role: revoke it first"),
          { status: 409 },
        );
      }
      roles.splice(index, 1);
      return new HttpResponse(null, { status: 204 });
    }),
  ];
  return { handlers, requests, ifMatches };
}

/** `access` as the server answers a caller who isn't an admin. */
export const notAnAdmin = [
  http.get("*/api/access/catalog", forbidden),
  http.get("*/api/access/grants", forbidden),
  http.get("*/api/access/roles", forbidden),
];
