import type {
  CatalogResponse,
  ErrorBody,
  GrantRequest,
  GrantResponse,
  ListResultGrantResponse,
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

function refusal(code: ErrorBody["code"], message: string, path?: string): ErrorBody {
  return { code, message, ...(path && { fields: [{ path, message }] }), requestId: "test" };
}

const forbidden = () => HttpResponse.json(refusal("forbidden", "forbidden"), { status: 403 });

/**
 * `access` as the server answers an admin for `items` (changed in place by grants and
 * revocations): the catalog, the grants filtered by role, resource and action, a grant checked
 * against the catalog and refused when equal to one made, and revocations guarded by `If-Match`.
 * `requests` records each list's query and `ifMatches` each revocation's `If-Match`.
 */
export function accessApi(items: GrantResponse[]) {
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
  ];
  return { handlers, requests, ifMatches };
}

/** `access` as the server answers a caller who isn't an admin. */
export const notAnAdmin = [
  http.get("*/api/access/catalog", forbidden),
  http.get("*/api/access/grants", forbidden),
];
