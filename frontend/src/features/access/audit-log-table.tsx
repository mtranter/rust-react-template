import { getAccessAuditOptions } from "@/api/@tanstack/react-query.gen";
import type { AuditChangeParam, AuditEntryResponse, AuditSort } from "@/api/types.gen";
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
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { isUnavailable } from "@/lib/api-errors";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { type Labels, useCatalog } from "./catalog";
import { LoadFailed } from "./load-failed";
import { useRoles } from "./use-roles";

export const auditChanges: readonly AuditChangeParam[] = [
  "accessGranted",
  "grantRevoked",
  "roleCreated",
  "roleDeleted",
  "roleAssigned",
  "roleUnassigned",
];

const changeLabels: Record<AuditChangeParam, string> = {
  accessGranted: "Access granted",
  grantRevoked: "Access revoked",
  roleCreated: "Role created",
  roleDeleted: "Role deleted",
  roleAssigned: "Role assigned",
  roleUnassigned: "Role unassigned",
};

export const auditSortKeys: readonly SortKey<AuditSort>[] = ["occurredAt"];

/**
 * Which entries to list. `since` and `until` are days (`YYYY-MM-DD`) in the viewer's time zone,
 * both included.
 */
export type AuditFilter = {
  change?: AuditChangeParam;
  role?: string;
  actor?: string;
  subject?: string;
  since?: string;
  until?: string;
};

function startOfDay(day: string, plusDays = 0): string | undefined {
  const at = new Date(`${day}T00:00`);
  if (Number.isNaN(at.getTime())) return undefined;
  at.setDate(at.getDate() + plusDays);
  return at.toISOString();
}

const column = createDataTableColumnHelper<AuditEntryResponse>();

function what(entry: AuditEntryResponse, labels: Labels): string {
  if (entry.resource && entry.action) {
    const scope = entry.scope ? ` (${labels.scope(entry.resource, entry.scope)})` : "";
    return `${labels.action(entry.resource, entry.action)} on ${labels.resource(entry.resource)}${scope}`;
  }
  return entry.subject ? `To ${entry.subject}` : "";
}

function columnsFor(labels: Labels) {
  return column.columns([
    column.accessor("occurredAt", {
      header: "When",
      enableHiding: false,
      cell: (info) => {
        const at = new Date(info.getValue());
        return <span title={at.toISOString()}>{at.toLocaleString()}</span>;
      },
    }),
    column.accessor("change", {
      header: "Change",
      enableHiding: false,
      enableSorting: false,
      cell: (info) => (
        <Badge
          variant={
            info.getValue() === "grantRevoked" ||
            info.getValue() === "roleDeleted" ||
            info.getValue() === "roleUnassigned"
              ? "outline"
              : "secondary"
          }
        >
          {changeLabels[info.getValue()] ?? info.getValue()}
        </Badge>
      ),
    }),
    column.accessor("role", {
      header: "Role",
      enableSorting: false,
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
    column.display({
      id: "detail",
      header: "Detail",
      cell: (info) => what(info.row.original, labels),
    }),
    column.accessor("actor", {
      header: "By",
      enableSorting: false,
      cell: (info) => info.getValue() ?? "System",
    }),
  ]);
}

/** The audit log of changes to who may do what, newest first unless sorted otherwise. */
export function AuditLogTable({
  page,
  sort,
  filter,
  onChange,
  onFilter,
}: ListPosition<AuditSort> & {
  filter: AuditFilter;
  onChange: (position: ListPosition<AuditSort>) => void;
  onFilter: (filter: AuditFilter) => void;
}) {
  const { labels } = useCatalog();
  const columns = useMemo(() => columnsFor(labels), [labels]);
  const params = useListParams<AuditSort>({ page, sort, onChange, sortable: auditSortKeys });
  const { since, until, ...exact } = filter;
  const entries = useQuery({
    ...getAccessAuditOptions({
      query: {
        ...params.query,
        sort: params.query.sort ?? ["-occurredAt"],
        ...exact,
        since: since && startOfDay(since),
        until: until && startOfDay(until, 1),
      },
    }),
    placeholderData: keepPreviousData,
  });
  const table = useDataTable({
    columns,
    result: entries.data,
    params,
    getRowId: (entry) => entry.id,
  });

  return (
    <section className="space-y-3" aria-label="Audit log">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <AuditFilters filter={filter} onFilter={onFilter} />
        <DataTableViewOptions table={table} />
      </div>
      {entries.isError ? (
        isUnavailable(entries.error) ? (
          <p role="alert" className="text-muted-foreground">
            The audit log is still being built. Try again shortly.
          </p>
        ) : (
          <LoadFailed error={entries.error} what="the audit log" />
        )
      ) : (
        <DataTable table={table} isLoading={entries.isPending} empty="No changes." />
      )}
      <DataTablePagination table={table} />
    </section>
  );
}

function AuditFilters({
  filter,
  onFilter,
}: {
  filter: AuditFilter;
  onFilter: (filter: AuditFilter) => void;
}) {
  const roles = useRoles();
  const roleNames = (roles.data?.items ?? []).map((r) => r.name);
  if (filter.role && !roleNames.includes(filter.role)) roleNames.push(filter.role);

  return (
    <fieldset className="flex flex-wrap gap-2">
      <legend className="sr-only">Filter the audit log</legend>
      <NativeSelect
        aria-label="Filter by change"
        className="w-40"
        value={filter.change ?? ""}
        onChange={(e) =>
          onFilter({
            ...filter,
            change: auditChanges.find((c) => c === e.target.value),
          })
        }
      >
        <NativeSelectOption value="">Every change</NativeSelectOption>
        {auditChanges.map((c) => (
          <NativeSelectOption key={c} value={c}>
            {changeLabels[c]}
          </NativeSelectOption>
        ))}
      </NativeSelect>
      <NativeSelect
        aria-label="Filter by role"
        className="w-40"
        value={filter.role ?? ""}
        onChange={(e) => onFilter({ ...filter, role: e.target.value || undefined })}
      >
        <NativeSelectOption value="">All roles</NativeSelectOption>
        {roleNames.map((name) => (
          <NativeSelectOption key={name} value={name}>
            {name}
          </NativeSelectOption>
        ))}
      </NativeSelect>
      <TextFilter
        label="Filter by who made it"
        placeholder="By (subject)"
        value={filter.actor}
        onChange={(actor) => onFilter({ ...filter, actor })}
      />
      <TextFilter
        label="Filter by whom it was assigned to"
        placeholder="To (subject)"
        value={filter.subject}
        onChange={(subject) => onFilter({ ...filter, subject })}
      />
      <Input
        type="date"
        aria-label="From"
        className="w-40"
        value={filter.since ?? ""}
        onChange={(e) => onFilter({ ...filter, since: e.target.value || undefined })}
      />
      <Input
        type="date"
        aria-label="Until"
        className="w-40"
        value={filter.until ?? ""}
        onChange={(e) => onFilter({ ...filter, until: e.target.value || undefined })}
      />
    </fieldset>
  );
}

/** An exact-match text filter, applied when the user leaves it or presses Enter. */
function TextFilter({
  label,
  placeholder,
  value,
  onChange,
}: {
  label: string;
  placeholder: string;
  value?: string;
  onChange: (value: string | undefined) => void;
}) {
  const [text, setText] = useState(value ?? "");
  useEffect(() => setText(value ?? ""), [value]);
  const apply = () => {
    const next = text.trim() || undefined;
    if (next !== value) onChange(next);
  };
  return (
    <Input
      aria-label={label}
      placeholder={placeholder}
      className="w-40"
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={apply}
      onKeyDown={(e) => {
        if (e.key === "Enter") apply();
      }}
    />
  );
}
