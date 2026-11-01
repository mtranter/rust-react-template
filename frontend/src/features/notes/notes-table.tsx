import { getNotesOptions } from "@/api/@tanstack/react-query.gen";
import type { NoteResponse, NoteSort } from "@/api/types.gen";
import {
  DataTable,
  DataTablePagination,
  DataTableViewOptions,
  createDataTableColumnHelper,
  useDataTable,
  useListParams,
} from "@/components/data-table";
import { keepPreviousData, useQuery } from "@tanstack/react-query";

const column = createDataTableColumnHelper<NoteResponse>();
const columns = column.columns([
  column.accessor("title", { header: "Title", enableHiding: false }),
  column.accessor("authorId", { header: "Author" }),
  column.accessor("createdAt", {
    header: "Created",
    cell: (info) => new Date(info.getValue()).toLocaleString(),
  }),
]);

export function NotesTable() {
  const params = useListParams<NoteSort>({ sortable: ["title", "createdAt"] });
  const notes = useQuery({
    ...getNotesOptions({ query: params.query }),
    placeholderData: keepPreviousData,
  });
  const table = useDataTable({
    columns,
    result: notes.data,
    params,
    getRowId: (note) => note.id,
  });

  return (
    <section className="space-y-3">
      <div className="flex justify-end">
        <DataTableViewOptions table={table} />
      </div>
      {notes.isError ? (
        <p role="alert" className="text-destructive">
          Failed to load notes.
        </p>
      ) : (
        <DataTable table={table} isLoading={notes.isPending} empty="No notes yet." />
      )}
      <DataTablePagination table={table} />
    </section>
  );
}
