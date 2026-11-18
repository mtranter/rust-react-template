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
import { http, HttpResponse, type RequestHandler } from "msw";
import type { Locator, Page } from "playwright";

// Read by both the browser entry (main.tsx) and the Node script (shoot.ts), which
// runs it with Node's type stripping: so only package imports, and types from `@/`.

export type Scenario = {
  /** The screenshot's file name: `<name>.<width>.png`. */
  name: string;
  route: string;
  handlers: RequestHandler[];
  /** What the user does before the shot, found by role and label as the Vitest tests do. */
  steps?: (page: Page) => Promise<void>;
  /** What the shot must show; the run fails if it never appears. */
  shows: (page: Page) => Locator;
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

const grants: GrantResponse[] = [
  ["user", "module:workshop", "use", "any"],
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
  http.get("*/api/access/grants", () =>
    HttpResponse.json({ items: grants, total: grants.length, limit: 20, offset: 0 }),
  ),
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
    name: "notes",
    route: "/",
    handlers: notesPage,
    shows: (page) => page.getByRole("cell", { name: "Shopping list", exact: true }),
  },
  {
    name: "notes-empty",
    route: "/",
    handlers: [listing([])],
    shows: (page) => page.getByText("No notes yet."),
  },
  {
    name: "notes-page-2-sorted",
    route: "/?page=2&sort=-title",
    handlers: notesPage,
    shows: (page) => page.getByText("21–25 of 25"),
  },
  {
    name: "notes-past-last-page",
    route: "/?page=4",
    handlers: notesPage,
    shows: (page) => page.getByText("Nothing on page 4."),
  },
  {
    name: "notes-dark",
    route: "/",
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
    route: "/",
    handlers: notesPage,
    steps: (page) => fillNewNote(page, "Dentist appointment"),
    shows: (page) => page.getByRole("button", { name: "Add note" }),
  },
  {
    name: "new-note-refused",
    route: "/",
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
    route: "/",
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
    route: "/",
    handlers: notesPage,
    steps: async (page) => {
      await openEditor(page);
    },
    shows: (page) => page.getByRole("dialog", { name: "Edit note" }).getByLabel("Body"),
  },
  {
    name: "edit-note-refused",
    route: "/",
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
    route: "/",
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
    route: "/search?q=alice",
    handlers: [searching(summaries)],
    shows: (page) => page.getByRole("cell", { name: "Shopping list", exact: true }),
  },
  {
    name: "search-no-match",
    route: "/search?q=zebra",
    handlers: [searching(summaries)],
    shows: (page) => page.getByText("No notes match."),
  },
  {
    name: "search-catching-up",
    route: "/search?q=alice",
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
    name: "access",
    route: "/access",
    handlers: accessApi,
    shows: (page) => page.getByText("Stale"),
  },
  {
    name: "access-grant",
    route: "/access",
    handlers: accessApi,
    steps: async (page) => {
      await page.getByRole("button", { name: "Grant access" }).click();
      const dialog = page.getByRole("dialog", { name: "Grant access" });
      await dialog.getByLabel("Role").selectOption("support");
      await dialog.getByLabel("Resource").selectOption("gadget");
    },
    shows: (page) => page.getByRole("dialog", { name: "Grant access" }).getByLabel("Action"),
  },
  {
    name: "access-roles",
    route: "/access?tab=roles",
    handlers: accessApi,
    shows: (page) => page.getByText("Answers customers"),
  },
  {
    name: "access-role-taken",
    route: "/access?tab=roles",
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
    name: "access-users",
    route: "/access?tab=users",
    handlers: accessApi,
    shows: (page) => page.getByText("Bootstrap admin"),
  },
  {
    name: "access-users-assign-refused",
    route: "/access?tab=users",
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
    route: "/access?tab=users",
    handlers: accessApi,
    steps: async (page) => {
      await page.getByRole("button", { name: "Add user" }).click();
      await page.getByRole("dialog", { name: "Add user" }).getByLabel("Subject").fill("auth0|new");
    },
    shows: (page) => page.getByRole("dialog", { name: "Add user" }).getByLabel("Role"),
  },
  {
    name: "access-not-an-admin",
    route: "/access",
    handlers: [],
    shows: (page) => page.getByRole("alert"),
  },
];
