import { getNoteSearchOptions } from "@/api/@tanstack/react-query.gen";
import type { NoteSummaryResponse, NoteSummarySort } from "@/api/types.gen";
import {
  DataTable,
  DataTablePagination,
  DataTableViewOptions,
  createDataTableColumnHelper,
  useDataTable,
  useListParams,
} from "@/components/data-table";
import { Input } from "@/components/ui/input";
import { keepPreviousData, useQuery } from "@tanstack/react-query";

const column = createDataTableColumnHelper<NoteSummaryResponse>();
const columns = column.columns([
  column.accessor("title", { header: "Title", enableHiding: false }),
  column.accessor("authorName", {
    header: "Author",
    cell: (info) =>
      info.getValue() ?? <span className="text-muted-foreground">Unknown author</span>,
  }),
  column.accessor("createdAt", {
    header: "Created",
    cell: (info) => new Date(info.getValue()).toLocaleString(),
  }),
]);

/** `text` is controlled so the page can keep it in the URL. */
export function NoteSearch({
  text,
  onTextChange,
}: {
  text: string;
  onTextChange: (text: string) => void;
}) {
  const params = useListParams<NoteSummarySort>({
    sortable: ["title", "authorName", "createdAt"],
  });
  const found = useQuery({
    ...getNoteSearchOptions({ query: { ...params.query, ...(text.trim() && { q: text }) } }),
    placeholderData: keepPreviousData,
  });
  const table = useDataTable({
    columns,
    result: found.data,
    params,
    getRowId: (note) => note.id,
  });

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between gap-3">
        <Input
          type="search"
          aria-label="Search notes"
          placeholder="Search by title or author…"
          className="max-w-sm"
          value={text}
          onChange={(e) => {
            params.onPaginationChange((p) => ({ ...p, pageIndex: 0 }));
            onTextChange(e.target.value);
          }}
        />
        <DataTableViewOptions table={table} />
      </div>
      {found.isError ? (
        <p role="alert" className="text-destructive">
          Failed to search notes.
        </p>
      ) : (
        <DataTable
          table={table}
          isLoading={found.isPending}
          empty={text.trim() ? "No notes match." : "No notes yet."}
        />
      )}
      <DataTablePagination table={table} />
    </section>
  );
}
