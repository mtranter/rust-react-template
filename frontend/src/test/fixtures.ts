import type {
  EditNoteRequest,
  ErrorBody,
  ListResultNoteResponse,
  ListResultUserResponse,
  NoteResponse,
  UserResponse,
} from "@/api";
import { http, HttpResponse } from "msw";

export const alice: UserResponse = {
  id: "a1",
  email: "alice@example.com",
  displayName: "Alice",
  createdAt: "2026-01-01T00:00:00Z",
  version: "1",
};

export function note(n: number, changes: Partial<NoteResponse> = {}): NoteResponse {
  return {
    id: `note-${n}`,
    authorId: alice.id,
    title: `Note ${n}`,
    body: "",
    createdAt: new Date(Date.UTC(2026, 0, n)).toISOString(),
    items: [],
    version: "1",
    ...changes,
  };
}

export function notes(count: number) {
  return Array.from({ length: count }, (_, i) => note(i + 1));
}

/** A page of `items` by the request's `limit` and `offset`, as the server answers. */
function pageOf<T>(items: T[], request: Request) {
  const params = new URL(request.url).searchParams;
  const limit = Number(params.get("limit") ?? 20);
  const offset = Number(params.get("offset") ?? 0);
  return { items: items.slice(offset, offset + limit), total: items.length, limit, offset };
}

function sorted(items: NoteResponse[], sort: string | null) {
  if (!sort) return items;
  const key = sort.replace(/^-/, "") as "title" | "createdAt";
  const ordered = [...items].sort((a, b) => a[key].localeCompare(b[key]));
  return sort.startsWith("-") ? ordered.reverse() : ordered;
}

/** Records the `offset` and `sort` asked of the list at `path`, leaving the answer to the handlers after it. */
export function listRequests(path: string) {
  const requests: { offset: string | null; sort: string | null }[] = [];
  return {
    requests,
    handler: http.get(path, ({ request }) => {
      const params = new URL(request.url).searchParams;
      requests.push({ offset: params.get("offset"), sort: params.get("sort") });
    }),
  };
}

export function refusal(code: ErrorBody["code"], message: string, path?: string): ErrorBody {
  return { code, message, ...(path && { fields: [{ path, message }] }), requestId: "test" };
}

/**
 * The API as the server answers it for `items` (changed in place by edits): the notes list,
 * each note with its `ETag`, edits guarded by `If-Match`, and Alice as the only user.
 * `ifMatches` records each edit's `If-Match`.
 */
export function notesApi(items: NoteResponse[]) {
  const ifMatches: (string | null)[] = [];
  const handlers = [
    http.get("*/api/users", () =>
      HttpResponse.json({ items: [alice] } satisfies ListResultUserResponse),
    ),
    http.get("*/api/notes", ({ request }) => {
      const sort = new URL(request.url).searchParams.get("sort");
      return HttpResponse.json(
        pageOf(sorted(items, sort), request) satisfies ListResultNoteResponse,
      );
    }),
    http.get("*/api/notes/:id", ({ params }) => {
      const found = items.find((n) => n.id === params.id);
      if (!found) return HttpResponse.json(refusal("not_found", "note not found"), { status: 404 });
      return HttpResponse.json(found, { headers: { ETag: `"${found.version}"` } });
    }),
    http.patch("*/api/notes/:id", async ({ params, request }) => {
      const index = items.findIndex((n) => n.id === params.id);
      if (index < 0) {
        return HttpResponse.json(refusal("not_found", "note not found"), { status: 404 });
      }
      const ifMatch = request.headers.get("If-Match");
      ifMatches.push(ifMatch);
      const current = items[index];
      if (ifMatch !== `"${current.version}"`) {
        return HttpResponse.json(
          refusal("precondition_failed", "the note has changed since it was read"),
          { status: 412 },
        );
      }
      const edit = (await request.json()) as EditNoteRequest;
      const edited = { ...current, ...edit, version: String(Number(current.version) + 1) };
      items[index] = edited;
      return HttpResponse.json(edited, { headers: { ETag: `"${edited.version}"` } });
    }),
  ];
  return { handlers, ifMatches };
}
