import { getNotesOptions } from "@/api/@tanstack/react-query.gen";
import type { NoteResponse, NoteSort } from "@/api/types.gen";
import {
  DataTable,
  DataTablePagination,
  DataTableViewOptions,
  type ListPosition,
  type SortKey,
  createDataTableColumnHelper,
  useDataTable,
  useListParams,
} from "@/components/data-table";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { EditNoteDialog } from "./edit-note-dialog";

const column = createDataTableColumnHelper<NoteResponse>();
const columns = column.columns([
  column.accessor("title", { header: "Title", enableHiding: false }),
  column.accessor("authorId", { header: "Author" }),
  column.accessor("createdAt", {
    header: "Created",
    cell: (info) => new Date(info.getValue()).toLocaleString(),
  }),
  column.display({
    id: "actions",
    header: () => <span className="sr-only">Actions</span>,
    enableHiding: false,
    // An archived note can't be edited.
    cell: (info) =>
      info.row.original.archivedAt ? null : <EditNoteDialog note={info.row.original} />,
  }),
]);

export const noteSortKeys: readonly SortKey<NoteSort>[] = ["title", "createdAt"];

export function NotesTable({
  page,
  sort,
  onChange,
}: ListPosition<NoteSort> & { onChange: (position: ListPosition<NoteSort>) => void }) {
  const params = useListParams<NoteSort>({ page, sort, onChange, sortable: noteSortKeys });
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
    <section className="space-y-3" aria-label="Notes">
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
