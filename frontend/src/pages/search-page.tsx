import { NoteSearch } from "@/features/note-search";
import { Link, getRouteApi } from "@tanstack/react-router";

const route = getRouteApi("/search");

export function SearchPage() {
  const { q } = route.useSearch();
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
        onTextChange={(text) => navigate({ search: { q: text || undefined }, replace: true })}
      />
    </main>
  );
}
