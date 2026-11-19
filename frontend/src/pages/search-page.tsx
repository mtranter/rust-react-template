import { listSearch } from "@/components/data-table";
import { NoteSearch } from "@/features/note-search";
import { Link, getRouteApi } from "@tanstack/react-router";

const route = getRouteApi("/app/search");

export function SearchPage() {
  const { q, page = 1, sort } = route.useSearch();
  const navigate = route.useNavigate();

  return (
    <main className="mx-auto max-w-4xl space-y-6 p-6">
      <div className="flex items-baseline justify-between">
        <h1 className="font-heading text-2xl font-semibold">Search notes</h1>
        <Link to="/" className="text-sm underline-offset-4 hover:underline">
          All notes
        </Link>
      </div>
      <NoteSearch
        text={q ?? ""}
        onTextChange={(text) =>
          navigate({
            search: (prev) => ({ ...prev, q: text || undefined, page: undefined }),
            replace: true,
          })
        }
        page={page}
        sort={sort}
        onChange={(position) =>
          navigate({ search: (prev) => ({ ...prev, ...listSearch(position) }) })
        }
      />
    </main>
  );
}
