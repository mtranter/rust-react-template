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
import {
  NativeSelect,
  NativeSelectOptGroup,
  NativeSelectOption,
} from "@/components/ui/native-select";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useMemo } from "react";
import { type Labels, byModule, isGate, useCatalog } from "./catalog";
import { RevokeGrantDialog } from "./revoke-grant-dialog";
import { useRoles } from "./use-roles";

const column = createDataTableColumnHelper<GrantResponse>();

function columnsFor(labels: Labels) {
  return column.columns([
    column.accessor("role", {
      header: "Role",
      enableHiding: false,
      cell: (info) => (
        <Link
          to="/access/roles/$role"
          params={{ role: info.getValue() }}
          className="font-medium underline-offset-4 hover:underline"
        >
          {info.getValue()}
        </Link>
      ),
    }),
    column.accessor("action", {
      header: "Can",
      enableHiding: false,
      cell: (info) => (
        <span className="flex items-center gap-2">
          {labels.action(info.row.original.resource, info.getValue())}
          {info.row.original.stale && (
            <Badge
              variant="destructive"
              title="Its module no longer has this, so it allows nothing: safe to revoke"
            >
              No longer used
            </Badge>
          )}
        </span>
      ),
    }),
    column.accessor("resource", {
      header: "On",
      enableHiding: false,
      cell: (info) => labels.resource(info.getValue()),
    }),
    column.accessor("scope", {
      header: "Which",
      // A module's area has no rows to choose between.
      cell: (info) =>
        isGate(info.row.original.resource)
          ? null
          : labels.scope(info.row.original.resource, info.getValue()),
    }),
    column.accessor("grantedBy", {
      header: "Granted by",
      cell: (info) => info.getValue() ?? "Built in",
    }),
    column.accessor("grantedAt", {
      header: "Granted",
      cell: (info) => {
        const at = new Date(info.getValue());
        return <span title={at.toLocaleString()}>{at.toLocaleDateString()}</span>;
      },
    }),
    column.display({
      id: "actions",
      header: () => <span className="sr-only">Actions</span>,
      enableHiding: false,
      cell: (info) => <RevokeGrantDialog grant={info.row.original} labels={labels} />,
    }),
  ]);
}

export const grantSortKeys: readonly SortKey<GrantSort>[] = [
  "role",
  "resource",
  "action",
  "grantedAt",
];

/** Which grants to list: each given field matches exactly. */
export type GrantFilter = { role?: string; resource?: string; action?: string };

/** Every grant, of every role, filtered by role, resource and action. */
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
  const { labels } = useCatalog();
  const columns = useMemo(() => columnsFor(labels), [labels]);
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
    <section className="space-y-3" aria-label="Permissions">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <GrantFilters filter={filter} onFilter={onFilter} />
        <DataTableViewOptions table={table} />
      </div>
      {grants.isError ? (
        <p role="alert" className="text-destructive">
          Failed to load the permissions.
        </p>
      ) : (
        <DataTable table={table} isLoading={grants.isPending} empty="No permissions." />
      )}
      <DataTablePagination table={table} />
    </section>
  );
}

type Option = { value: string; label: string; group?: string };

function GrantFilters({
  filter,
  onFilter,
}: {
  filter: GrantFilter;
  onFilter: (filter: GrantFilter) => void;
}) {
  const roles = useRoles();
  const { resources, labels } = useCatalog();
  const actionsOf = (resource?: string) =>
    resources.find((r) => r.resource === resource)?.actions.map((a) => a.name) ?? [];

  return (
    <fieldset className="flex flex-wrap gap-2">
      <legend className="sr-only">Filter permissions</legend>
      <FilterSelect
        label="Role"
        all="All roles"
        value={filter.role}
        options={(roles.data?.items ?? []).map((r) => ({ value: r.name, label: r.name }))}
        onChange={(role) => onFilter({ ...filter, role })}
      />
      <FilterSelect
        label="Resource"
        all="Everything"
        value={filter.resource}
        options={byModule(resources).flatMap((m) =>
          m.resources.map((r) => ({ value: r.resource, label: r.label, group: m.label })),
        )}
        onChange={(resource) =>
          onFilter({
            ...filter,
            resource,
            action:
              filter.action && actionsOf(resource).includes(filter.action)
                ? filter.action
                : undefined,
          })
        }
      />
      <FilterSelect
        label="Action"
        all={filter.resource ? "Any action" : "Choose what first"}
        disabled={!filter.resource && !filter.action}
        value={filter.action}
        options={actionsOf(filter.resource).map((a) => ({
          value: a,
          label: filter.resource ? labels.action(filter.resource, a) : a,
        }))}
        onChange={(action) => onFilter({ ...filter, action })}
      />
    </fieldset>
  );
}

/**
 * A filter offering `options`, grouped by each one's `group`. A value not among them, such as a
 * stale grant's action or one linked to, stays chosen and listed rather than silently dropped.
 */
function FilterSelect({
  label,
  all,
  value,
  options,
  disabled,
  onChange,
}: {
  label: string;
  all: string;
  value?: string;
  options: Option[];
  disabled?: boolean;
  onChange: (value: string | undefined) => void;
}) {
  const offered =
    value && !options.some((o) => o.value === value)
      ? [...options, { value, label: value }]
      : options;
  const groups = [...new Set(offered.map((o) => o.group))];
  const render = (o: Option) => (
    <NativeSelectOption key={o.value} value={o.value}>
      {o.label}
    </NativeSelectOption>
  );
  return (
    <NativeSelect
      aria-label={`Filter by ${label.toLowerCase()}`}
      className="w-48"
      disabled={disabled}
      value={value ?? ""}
      onChange={(event) => onChange(event.target.value || undefined)}
    >
      <NativeSelectOption value="">{all}</NativeSelectOption>
      {groups.map((group) =>
        group === undefined ? (
          offered.filter((o) => o.group === undefined).map(render)
        ) : (
          <NativeSelectOptGroup key={group} label={group}>
            {offered.filter((o) => o.group === group).map(render)}
          </NativeSelectOptGroup>
        ),
      )}
    </NativeSelect>
  );
}
