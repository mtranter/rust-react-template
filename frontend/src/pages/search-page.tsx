import { listSearch } from "@/components/data-table";
import { Page } from "@/components/page";
import { NoteSearch } from "@/features/note-search";
import { getRouteApi } from "@tanstack/react-router";

const route = getRouteApi("/app/notes/search");

export function SearchPage() {
  const { q, page = 1, sort } = route.useSearch();
  const navigate = route.useNavigate();

  return (
    <Page title="Search notes">
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
    </Page>
  );
}
