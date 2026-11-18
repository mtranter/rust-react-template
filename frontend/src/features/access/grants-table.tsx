import { getAccessGrantsOptions } from "@/api/@tanstack/react-query.gen";
import type { GrantResponse, GrantSort } from "@/api/types.gen";
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
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { RevokeGrantDialog } from "./revoke-grant-dialog";

const column = createDataTableColumnHelper<GrantResponse>();
const columns = column.columns([
  column.accessor("role", { header: "Role", enableHiding: false }),
  column.accessor("resource", { header: "Resource", enableHiding: false }),
  column.accessor("action", {
    header: "Action",
    enableHiding: false,
    cell: (info) => (
      <span className="flex items-center gap-2">
        {info.getValue()}
        {info.row.original.stale && (
          <Badge variant="destructive" title="Its module no longer has this; it allows nothing">
            Stale
          </Badge>
        )}
      </span>
    ),
  }),
  column.accessor("scope", { header: "Scope" }),
  column.accessor("grantedBy", {
    header: "Granted by",
    cell: (info) => info.getValue() ?? "Default",
  }),
  column.accessor("grantedAt", {
    header: "Granted",
    cell: (info) => new Date(info.getValue()).toLocaleString(),
  }),
  column.display({
    id: "actions",
    header: () => <span className="sr-only">Actions</span>,
    enableHiding: false,
    cell: (info) => <RevokeGrantDialog grant={info.row.original} />,
  }),
]);

export const grantSortKeys: readonly SortKey<GrantSort>[] = [
  "role",
  "resource",
  "action",
  "grantedAt",
];

/** Which grants to list: each given field matches exactly. */
export type GrantFilter = { role?: string; resource?: string; action?: string };

const filterFields = [
  ["role", "Role"],
  ["resource", "Resource"],
  ["action", "Action"],
] as const;

export function GrantsTable({
  page,
  sort,
  filter,
  onChange,
  onFilter,
}: ListPosition<GrantSort> & {
  filter: GrantFilter;
  onChange: (position: ListPosition<GrantSort>) => void;
  onFilter: (filter: GrantFilter) => void;
}) {
  const params = useListParams<GrantSort>({ page, sort, onChange, sortable: grantSortKeys });
  const grants = useQuery({
    ...getAccessGrantsOptions({ query: { ...params.query, ...filter } }),
    placeholderData: keepPreviousData,
  });
  const table = useDataTable({
    columns,
    result: grants.data,
    params,
    getRowId: (grant) => grant.id,
  });

  return (
    <section className="space-y-3" aria-label="Grants">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <fieldset className="flex flex-wrap gap-2">
          <legend className="sr-only">Filter grants</legend>
          {filterFields.map(([field, label]) => (
            <Input
              key={field}
              aria-label={`Filter by ${label.toLowerCase()}`}
              placeholder={label}
              className="w-36"
              defaultValue={filter[field] ?? ""}
              onKeyDown={(event) => {
                if (event.key !== "Enter") return;
                const value = event.currentTarget.value.trim();
                onFilter({ ...filter, [field]: value || undefined });
              }}
              onBlur={(event) => {
                const value = event.currentTarget.value.trim() || undefined;
                if (value !== filter[field]) onFilter({ ...filter, [field]: value });
              }}
            />
          ))}
        </fieldset>
        <DataTableViewOptions table={table} />
      </div>
      {grants.isError ? (
        <p role="alert" className="text-destructive">
          Failed to load the grants.
        </p>
      ) : (
        <DataTable table={table} isLoading={grants.isPending} empty="No grants." />
      )}
      <DataTablePagination table={table} />
    </section>
  );
}
