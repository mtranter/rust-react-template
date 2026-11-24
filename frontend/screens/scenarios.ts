import type {
  CatalogResponse,
  ErrorBody,
  GrantResponse,
  ListResultNoteResponse,
  ListResultNoteSummaryResponse,
  NoteResponse,
  NoteSummaryResponse,
  RoleResponse,
  SubjectResponse,
  UserResponse,
} from "@/api";
import type { SignInProvider } from "@/lib/sign-in";
import { http, HttpResponse, type RequestHandler } from "msw";
import type { Locator, Page } from "playwright";
import { assistantScenarios } from "./assistant-scenarios.ts";
import { signInScenarios } from "./sign-in-scenarios.ts";

// Read by both the browser entry (main.tsx) and the Node script (shoot.ts), which
// runs it with Node's type stripping: so only package imports, types from `@/`, and other scenarios.

export type Scenario = {
  /** The screenshot's file name: `<name>.<width>.png`. */
  name: string;
  route: string;
  handlers: RequestHandler[];
  /** What the user does before the shot, found by role and label as the Vitest tests do. */
  steps?: (page: Page) => Promise<void>;
  /** What the shot must show; the run fails if it never appears. */
  shows: (page: Page) => Locator;
  /** The identity provider, when the app signs people in; with none it signs no one in, as in dev. */
  signIn?: () => SignInProvider;
};

const users: UserResponse[] = [
  ["Alice", "alice@example.com"],
  ["Bob", "bob@example.com"],
].map(([displayName, email], i) => ({
  id: `00000000-0000-0000-0000-${String(i + 1).padStart(12, "0")}`,
  email,
  displayName,
  createdAt: new Date(Date.UTC(2026, 0, 1, 9, 0)).toISOString(),
  version: "1",
}));

const titles = [
  "Shopping list",
  "Bike repairs",
  "Book club picks",
  "Holiday packing",
  "Garden plan",
  "Meeting notes",
  "Recipe ideas",
  "Birthday gifts",
  "Workout routine",
  "Reading list",
  "Car service",
  "Budget review",
  "Weekend plans",
  "Movie night",
  "House chores",
  "Podcast queue",
  "Language practice",
  "Camping gear",
  "Tax documents",
  "Conference talks",
  "Plant care",
  "Home office setup",
  "Board games",
  "Coffee beans to try",
  "Photo album",
];

const notes: NoteResponse[] = titles.map((title, i) => ({
  id: `00000000-0000-0000-0001-${String(i + 1).padStart(12, "0")}`,
  authorId: users[i % users.length].id,
  title,
  body: i === 0 ? "Milk, eggs, bread" : "",
  items: [],
  createdAt: new Date(Date.UTC(2026, 0, 5 + i, 9, 30)).toISOString(),
  version: "1",
}));

function refusal(code: ErrorBody["code"], message: string, path?: string): ErrorBody {
  return { code, message, ...(path && { fields: [{ path, message }] }), requestId: "screens" };
}

/** Answers the list as the server does: a page of `items` by `limit`, `offset` and `sort`. */
function listing(items: NoteResponse[]) {
  return http.get("*/api/notes", ({ request }) => {
    const params = new URL(request.url).searchParams;
    const limit = Number(params.get("limit") ?? 20);
    const offset = Number(params.get("offset") ?? 0);
    const sort = params.get("sort");
    const ordered = [...items];
    if (sort) {
      const key = sort.replace(/^-/, "") as "title" | "createdAt";
      ordered.sort((a, b) => a[key].localeCompare(b[key]));
      if (sort.startsWith("-")) ordered.reverse();
    }
    const page: ListResultNoteResponse = {
      items: ordered.slice(offset, offset + limit),
      total: items.length,
      limit,
      offset,
    };
    return HttpResponse.json(page);
  });
}

const notesPage = [listing(notes)];

// The server's message: `MAX_TITLE_LEN` in crates/reference/notes/core/src/domain/note.rs.
const titleTooLong = "title must be at most 200 characters";

/** Types `title` into the New note form; the signed-in caller is its author. */
async function fillNewNote(page: Page, title: string) {
  await page.getByLabel("Title").fill(title);
}

/** Opens the Edit note dialog from the first note's row. */
async function openEditor(page: Page) {
  await page.getByRole("button", { name: `Edit ${notes[0].title}` }).click();
  return page.getByRole("dialog", { name: "Edit note" });
}

/** Opens the editor, retitles the note and saves. */
async function saveEdit(page: Page, title: string) {
  const dialog = await openEditor(page);
  await dialog.getByLabel("Title").fill(title);
  await dialog.getByRole("button", { name: "Save" }).click();
  return dialog;
}

const summaries: NoteSummaryResponse[] = notes.map((note) => ({
  id: note.id,
  title: note.title,
  authorId: note.authorId,
  authorName: users.find((u) => u.id === note.authorId)?.displayName,
  createdAt: note.createdAt,
}));

function searching(items: NoteSummaryResponse[]) {
  return http.get("*/api/note-search", ({ request }) => {
    const q = new URL(request.url).searchParams.get("q")?.toLowerCase() ?? "";
    const found = items.filter((n) => `${n.title} ${n.authorName}`.toLowerCase().includes(q));
    return HttpResponse.json({
      items: found.slice(0, 20),
      total: found.length,
      limit: 20,
      offset: 0,
    } satisfies ListResultNoteSummaryResponse);
  });
}

const accessCatalog: CatalogResponse = {
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

const grants: GrantResponse[] = [
  ["user", "widget", "build", "any"],
  ["user", "gadget", "make", "any"],
  ["user", "gadget", "view", "maker"],
  ["admin", "gadget", "view", "any"],
  ["support", "gadget", "polish", "any"],
].map(([role, resource, action, scope], i) => ({
  id: `grant-${i + 1}`,
  role,
  resource,
  action,
  scope,
  ...(role === "support" && { grantedBy: "an-admin" }),
  grantedAt: new Date(Date.UTC(2026, 0, 1 + i, 9, 0)).toISOString(),
  stale: action === "polish",
  version: "1",
}));

const roles: RoleResponse[] = [
  ["admin", "Administers the app: always reaches access, whatever is granted."],
  ["support", "Answers customers"],
  ["user", "Every signed-in user."],
].map(([name, description]) => ({
  name,
  description,
  builtIn: name !== "support",
  ...(name === "support" && { createdBy: "an-admin" }),
  createdAt: new Date(Date.UTC(2026, 0, 1, 9, 0)).toISOString(),
  version: "1",
}));

const subjects: SubjectResponse[] = [
  {
    subject: "auth0|6650a1",
    name: "Ada Lovelace",
    email: "ada@example.com",
    firstSeen: new Date(Date.UTC(2026, 0, 1, 9, 0)).toISOString(),
    lastSeen: new Date(Date.UTC(2026, 0, 5, 14, 0)).toISOString(),
    bootstrapAdmin: false,
    roles: [
      { role: "admin", version: "1" },
      { role: "support", version: "2" },
    ],
  },
  {
    subject: "auth0|6650b2",
    name: "Bob Babbage",
    email: "bob@engines.org",
    firstSeen: new Date(Date.UTC(2026, 0, 2, 9, 0)).toISOString(),
    lastSeen: new Date(Date.UTC(2026, 0, 2, 9, 0)).toISOString(),
    bootstrapAdmin: false,
    roles: [],
  },
  { subject: "auth0|6650c3", bootstrapAdmin: false, roles: [{ role: "support", version: "1" }] },
  { subject: "ops@example.com", bootstrapAdmin: true, roles: [{ role: "admin" }] },
];

/** `access` as the server answers an admin. */
const accessApi = [
  http.get("*/api/access/catalog", () => HttpResponse.json(accessCatalog)),
  http.get("*/api/access/grants", ({ request }) => {
    const params = new URL(request.url).searchParams;
    const matching = grants.filter((g) =>
      (["role", "resource", "action"] as const).every(
        (field) => !params.get(field) || g[field] === params.get(field),
      ),
    );
    return HttpResponse.json({ items: matching, total: matching.length, limit: 20, offset: 0 });
  }),
  http.get("*/api/access/roles", () =>
    HttpResponse.json({ items: roles, total: roles.length, limit: 100, offset: 0 }),
  ),
  http.get("*/api/access/subjects", () =>
    HttpResponse.json({ items: subjects, total: subjects.length, limit: 100, offset: 0 }),
  ),
  http.post("*/api/access/assignments", () =>
    HttpResponse.json(
      {
        code: "conflict",
        message: "that role is assigned to that subject already",
        requestId: "screens",
      },
      { status: 409 },
    ),
  ),
  http.post("*/api/access/roles", () =>
    HttpResponse.json(
      { code: "conflict", message: "a role of that name exists already", requestId: "screens" },
      { status: 409 },
    ),
  ),
];

export const scenarios: Scenario[] = [
  {
    name: "launcher",
    route: "/",
    handlers: [],
    shows: (page) => page.getByRole("region", { name: "Apps" }),
  },
  {
    name: "launcher-admin",
    route: "/",
    handlers: accessApi,
    shows: (page) => page.getByRole("region", { name: "Platform" }),
  },
  {
    name: "app-switcher",
    route: "/notes",
    handlers: notesPage,
    steps: async (page) => {
      await page.getByRole("cell", { name: "Shopping list", exact: true }).waitFor();
      // On a phone the sidebar, and its switcher, is a sheet the header's toggle opens.
      if (!(await page.getByRole("button", { name: "Switch app" }).isVisible())) {
        await page.getByRole("banner").getByRole("button", { name: "Toggle Sidebar" }).click();
      }
      await page.getByRole("button", { name: "Switch app" }).click();
    },
    shows: (page) => page.getByRole("menuitem", { name: "All apps" }),
  },
  {
    name: "go-to",
    route: "/notes",
    handlers: [...notesPage, ...accessApi],
    steps: async (page) => {
      await page.getByRole("button", { name: "Go to" }).click();
    },
    shows: (page) => page.getByRole("option", { name: "Permissions" }),
  },
  {
    // Collapsed to its icons on a desktop; opened as a sheet on a phone.
    name: "sidebar-toggled",
    route: "/notes",
    handlers: notesPage,
    steps: async (page) => {
      await page.getByRole("cell", { name: "Shopping list", exact: true }).waitFor();
      await page.getByRole("banner").getByRole("button", { name: "Toggle Sidebar" }).click();
    },
    shows: (page) => page.getByRole("button", { name: "Switch app" }),
  },
  {
    name: "notes",
    route: "/notes",
    handlers: notesPage,
    shows: (page) => page.getByRole("cell", { name: "Shopping list", exact: true }),
  },
  {
    name: "notes-empty",
    route: "/notes",
    handlers: [listing([])],
    shows: (page) => page.getByText("No notes yet."),
  },
  {
    name: "notes-page-2-sorted",
    route: "/notes?page=2&sort=-title",
    handlers: notesPage,
    shows: (page) => page.getByText("21–25 of 25"),
  },
  {
    name: "notes-past-last-page",
    route: "/notes?page=4",
    handlers: notesPage,
    shows: (page) => page.getByText("Nothing on page 4."),
  },
  {
    name: "notes-dark",
    route: "/notes",
    handlers: notesPage,
    steps: async (page) => {
      await page.getByRole("button", { name: "Theme" }).click();
      await page.getByRole("menuitemradio", { name: "Dark" }).click();
      await page.keyboard.press("Escape");
    },
    shows: (page) =>
      page.locator("html.dark").getByRole("cell", { name: "Shopping list", exact: true }),
  },
  {
    name: "new-note",
    route: "/notes",
    handlers: notesPage,
    steps: (page) => fillNewNote(page, "Dentist appointment"),
    shows: (page) => page.getByRole("button", { name: "Add note" }),
  },
  {
    name: "new-note-refused",
    route: "/notes",
    handlers: [
      http.post("*/api/notes", () =>
        HttpResponse.json(refusal("validation_error", titleTooLong, "title"), { status: 400 }),
      ),
      ...notesPage,
    ],
    steps: async (page) => {
      await fillNewNote(page, "A title the server refuses");
      await page.getByRole("button", { name: "Add note" }).click();
    },
    shows: (page) => page.getByText(titleTooLong),
  },
  {
    name: "new-note-failed",
    route: "/notes",
    handlers: [
      http.post("*/api/notes", () =>
        HttpResponse.json(refusal("internal_error", "internal error"), { status: 500 }),
      ),
      ...notesPage,
    ],
    steps: async (page) => {
      await fillNewNote(page, "Dentist appointment");
      await page.getByRole("button", { name: "Add note" }).click();
    },
    shows: (page) => page.getByText("Couldn't save"),
  },
  {
    name: "edit-note",
    route: "/notes",
    handlers: notesPage,
    steps: async (page) => {
      await openEditor(page);
    },
    shows: (page) => page.getByRole("dialog", { name: "Edit note" }).getByLabel("Body"),
  },
  {
    name: "edit-note-refused",
    route: "/notes",
    handlers: [
      http.patch("*/api/notes/:id", () =>
        HttpResponse.json(refusal("validation_error", titleTooLong, "title"), { status: 400 }),
      ),
      ...notesPage,
    ],
    steps: async (page) => {
      await saveEdit(page, "A title the server refuses");
    },
    shows: (page) => page.getByRole("dialog", { name: "Edit note" }).getByText(titleTooLong),
  },
  {
    name: "edit-note-conflict",
    route: "/notes",
    handlers: [
      http.patch("*/api/notes/:id", () =>
        HttpResponse.json(
          refusal("precondition_failed", "the note has changed since it was read"),
          { status: 412 },
        ),
      ),
      http.get("*/api/notes/:id", () =>
        HttpResponse.json(
          { ...notes[0], title: "Groceries", version: "2" },
          { headers: { ETag: '"2"' } },
        ),
      ),
      ...notesPage,
    ],
    steps: async (page) => {
      await saveEdit(page, "Weekly shop");
    },
    shows: (page) =>
      page.getByRole("dialog", { name: "Edit note" }).getByText(/Someone else changed this note/),
  },
  {
    name: "search",
    route: "/notes/search?q=alice",
    handlers: [searching(summaries)],
    shows: (page) => page.getByRole("cell", { name: "Shopping list", exact: true }),
  },
  {
    name: "search-no-match",
    route: "/notes/search?q=zebra",
    handlers: [searching(summaries)],
    shows: (page) => page.getByText("No notes match."),
  },
  {
    name: "search-catching-up",
    route: "/notes/search?q=alice",
    handlers: [
      http.get("*/api/note-search", () =>
        HttpResponse.json(
          { code: "unavailable", message: "this read model is still being built: retry later" },
          { status: 503 },
        ),
      ),
    ],
    shows: (page) => page.getByRole("alert"),
  },
  {
    name: "access-permissions",
    route: "/access/permissions",
    handlers: accessApi,
    shows: (page) => page.getByText("No longer used"),
  },
  {
    name: "access-grant",
    route: "/access/permissions",
    handlers: accessApi,
    steps: async (page) => {
      await page.getByRole("button", { name: "Grant access" }).click();
      const dialog = page.getByRole("dialog", { name: "Grant access" });
      await dialog.getByLabel("Role").selectOption("support");
      await dialog.getByLabel("On").selectOption("gadget");
    },
    shows: (page) => page.getByRole("dialog", { name: "Grant access" }).getByLabel("Can"),
  },
  {
    name: "access-role",
    route: "/access/roles/user",
    handlers: accessApi,
    shows: (page) => page.getByRole("combobox", { name: "Gadgets: View" }),
  },
  {
    name: "access-roles",
    route: "/access/roles",
    handlers: accessApi,
    shows: (page) => page.getByText("Answers customers"),
  },
  {
    name: "access-role-taken",
    route: "/access/roles",
    handlers: accessApi,
    steps: async (page) => {
      await page.getByRole("button", { name: "Create role" }).click();
      const dialog = page.getByRole("dialog", { name: "Create role" });
      await dialog.getByLabel("Name").fill("support");
      await dialog.getByRole("button", { name: "Create" }).click();
    },
    shows: (page) =>
      page.getByRole("dialog", { name: "Create role" }).getByText("a role of that name exists"),
  },
  {
    name: "access",
    route: "/access/subjects",
    handlers: accessApi,
    shows: (page) => page.getByText("Always admin"),
  },
  {
    name: "access-users-assign-refused",
    route: "/access/subjects",
    handlers: accessApi,
    steps: async (page) => {
      await page.getByRole("button", { name: "Assign a role to Ada Lovelace" }).click();
      const dialog = page.getByRole("dialog", { name: "Assign a role to Ada Lovelace" });
      await dialog.getByLabel("Role").selectOption("support");
      await dialog.getByRole("button", { name: "Assign" }).click();
    },
    shows: (page) =>
      page.getByRole("dialog", { name: "Assign a role to Ada Lovelace" }).getByText("already"),
  },
  {
    name: "access-add-user",
    route: "/access/subjects",
    handlers: accessApi,
    steps: async (page) => {
      await page.getByRole("button", { name: "Add user" }).click();
      await page.getByRole("dialog", { name: "Add user" }).getByLabel("User ID").fill("auth0|new");
    },
    shows: (page) => page.getByRole("dialog", { name: "Add user" }).getByLabel("Role"),
  },
  {
    name: "access-not-an-admin",
    route: "/access/subjects",
    handlers: [],
    shows: (page) => page.getByRole("alert"),
  },
  ...assistantScenarios,
  ...signInScenarios,
];
