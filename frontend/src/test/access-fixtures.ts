import type {
  AssignmentRequest,
  AssignmentResponse,
  CatalogResponse,
  ErrorBody,
  GrantRequest,
  GrantResponse,
  ListResultGrantResponse,
  ListResultRoleResponse,
  ListResultSubjectResponse,
  RoleRequest,
  RoleResponse,
  SubjectResponse,
} from "@/api";
import { everyAreasNeeds, holding } from "@/test/may-use-everything";
import { http, HttpResponse } from "msw";

/** What an admin may do: every Area, and every change `access` offers. */
export const adminPermissions = [
  ...everyAreasNeeds,
  { resource: "assignment", action: "assign" },
  { resource: "assignment", action: "unassign" },
  { resource: "grant", action: "create" },
  { resource: "grant", action: "revoke" },
  { resource: "role", action: "create" },
  { resource: "role", action: "delete" },
];

export const catalog: CatalogResponse = {
  capabilities: [
    {
      capability: "workshop",
      label: "Workshop",
      resources: [
        {
          module: "widgets",
          moduleLabel: "Widgets",
          resource: "widget",
          label: "Widgets",
          actions: [
            { name: "build", label: "Build", operations: ["build_widget"], reachable: true },
          ],
          scopes: [{ name: "any", label: "Any" }],
        },
        {
          module: "gadgets",
          moduleLabel: "Gadgets",
          resource: "gadget",
          label: "Gadgets",
          actions: [
            { name: "make", label: "Make", operations: ["make_gadget"], reachable: true },
            { name: "view", label: "View", operations: ["get_gadget"], reachable: true },
            { name: "scrap", label: "Scrap", operations: [], reachable: false },
          ],
          scopes: [
            { name: "any", label: "Every gadget" },
            { name: "maker", label: "Gadgets they made" },
          ],
        },
      ],
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

export function subject(id: string, changes: Partial<SubjectResponse> = {}): SubjectResponse {
  return {
    subject: id,
    firstSeen: new Date(Date.UTC(2026, 0, 1)).toISOString(),
    lastSeen: new Date(Date.UTC(2026, 0, 2)).toISOString(),
    bootstrapAdmin: false,
    roles: [],
    ...changes,
  };
}

/** Ada, who supports; Bob, who holds nothing; and Root, a bootstrap admin never signed in. */
export function someSubjects(): SubjectResponse[] {
  return [
    subject("auth0|ada", {
      name: "Ada Lovelace",
      email: "ada@example.com",
      roles: [{ role: "support", version: "2" }],
    }),
    subject("auth0|bob", { name: "Bob Babbage", email: "bob@engines.org" }),
    {
      subject: "root",
      bootstrapAdmin: true,
      roles: [{ role: "admin" }],
    },
  ];
}

function refusal(code: ErrorBody["code"], message: string, path?: string): ErrorBody {
  return { code, message, ...(path && { fields: [{ path, message }] }), requestId: "test" };
}

const forbidden = () => HttpResponse.json(refusal("forbidden", "forbidden"), { status: 403 });

/**
 * `access` as the server answers an admin for `items` and `roles` (changed in place): the
 * catalog, the grants filtered by role, resource and action and paged, a grant checked against the roles
 * and the catalog and refused when equal to one made, revocations guarded by `If-Match`, and the
 * roles, created unless the name is taken and deleted unless built in or granted; and the
 * `subjects`, searched by part of their subject, name or email, assigned roles that exist unless
 * held already (an unseen subject added), and unassigned with `If-Match` unless it's a bootstrap
 * admin's `admin`. `requests` records each grant and subject list's query and `ifMatches` each
 * revocation's, deletion's and unassignment's `If-Match`.
 */
export function accessApi(
  items: GrantResponse[],
  roles: RoleResponse[] = someRoles(),
  subjects: SubjectResponse[] = someSubjects(),
) {
  const requests: URLSearchParams[] = [];
  const ifMatches: (string | null)[] = [];
  const handlers = [
    holding(adminPermissions),
    http.get("*/api/access/catalog", () => HttpResponse.json(catalog)),
    http.get("*/api/access/grants", ({ request }) => {
      const params = new URL(request.url).searchParams;
      requests.push(params);
      const matching = items.filter((g) =>
        (["role", "resource", "action"] as const).every(
          (field) => !params.get(field) || g[field] === params.get(field),
        ),
      );
      const limit = Number(params.get("limit") ?? 20);
      const offset = Number(params.get("offset") ?? 0);
      return HttpResponse.json({
        items: matching.slice(offset, offset + limit),
        total: matching.length,
        limit,
        offset,
      } satisfies ListResultGrantResponse);
    }),
    http.post("*/api/access/grants", async ({ request }) => {
      const body = (await request.json()) as GrantRequest;
      if (!roles.some((r) => r.name === body.role)) {
        return HttpResponse.json(refusal("validation_error", "no such role", "role"), {
          status: 400,
        });
      }
      const resource = catalog.capabilities
        .flatMap((c) => c.resources)
        .find((r) => r.resource === body.resource);
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
    http.get("*/api/access/subjects", ({ request }) => {
      const params = new URL(request.url).searchParams;
      requests.push(params);
      const search = params.get("search")?.toLowerCase();
      const matching = subjects.filter(
        (s) =>
          !search ||
          [s.subject, s.name, s.email].some((field) => field?.toLowerCase().includes(search)),
      );
      return HttpResponse.json({
        items: matching,
        total: matching.length,
        limit: 100,
        offset: 0,
      } satisfies ListResultSubjectResponse);
    }),
    http.post("*/api/access/assignments", async ({ request }) => {
      const body = (await request.json()) as AssignmentRequest;
      if (!body.subject.trim()) {
        return HttpResponse.json(refusal("validation_error", "a subject is named", "subject"), {
          status: 400,
        });
      }
      if (body.role === "user" || !roles.some((r) => r.name === body.role)) {
        return HttpResponse.json(refusal("validation_error", "no role has that name", "role"), {
          status: 400,
        });
      }
      let held = subjects.find((s) => s.subject === body.subject);
      if (held?.roles.some((r) => r.role === body.role)) {
        return HttpResponse.json(
          refusal("conflict", "that role is assigned to that subject already"),
          { status: 409 },
        );
      }
      if (!held) {
        held = { subject: body.subject, bootstrapAdmin: false, roles: [] };
        subjects.push(held);
      }
      held.roles.push({ role: body.role, version: "1" });
      const made: AssignmentResponse = {
        subject: body.subject,
        role: body.role,
        assignedBy: "an-admin",
        assignedAt: new Date(Date.UTC(2026, 0, 3)).toISOString(),
        version: "1",
      };
      return HttpResponse.json(made, { headers: { ETag: `"${made.version}"` } });
    }),
    http.delete("*/api/access/assignments", ({ request }) => {
      const params = new URL(request.url).searchParams;
      const held = subjects.find((s) => s.subject === params.get("subject"));
      const index = held?.roles.findIndex((r) => r.role === params.get("role")) ?? -1;
      if (!held || index < 0) {
        return HttpResponse.json(refusal("not_found", "assignment not found"), { status: 404 });
      }
      const ifMatch = request.headers.get("If-Match");
      ifMatches.push(ifMatch);
      if (held.bootstrapAdmin && held.roles[index].role === "admin") {
        return HttpResponse.json(refusal("conflict", "a bootstrap admin always holds `admin`"), {
          status: 409,
        });
      }
      if (ifMatch !== `"${held.roles[index].version}"`) {
        return HttpResponse.json(refusal("precondition_failed", "the assignment has changed"), {
          status: 412,
        });
      }
      held.roles.splice(index, 1);
      return new HttpResponse(null, { status: 204 });
    }),
  ];
  return { handlers, requests, ifMatches };
}

const accessResources = ["subject", "role", "grant", "assignment"];

/** `access` as the server answers a caller who isn't an admin: they hold none of its actions. */
export const notAnAdmin = [
  holding(everyAreasNeeds.filter((needs) => !accessResources.includes(needs.resource))),
  http.get("*/api/access/catalog", forbidden),
  http.get("*/api/access/grants", forbidden),
  http.get("*/api/access/roles", forbidden),
  http.get("*/api/access/subjects", forbidden),
];
