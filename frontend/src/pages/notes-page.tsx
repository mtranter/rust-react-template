import { listSearch } from "@/components/data-table";
import { Page } from "@/components/page";
import { NewNoteForm, NotesTable } from "@/features/notes";
import { getRouteApi } from "@tanstack/react-router";

const route = getRouteApi("/app/notes/");

export function NotesPage() {
  const { page = 1, sort } = route.useSearch();
  const navigate = route.useNavigate();

  return (
    <Page title="Notes" description="Everyone's notes, newest first unless you sort them.">
      <NewNoteForm />
      <NotesTable
        page={page}
        sort={sort}
        onChange={(position) =>
          navigate({ search: (prev) => ({ ...prev, ...listSearch(position) }) })
        }
      />
    </Page>
  );
}
