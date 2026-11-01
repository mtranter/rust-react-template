import {
  type ColumnDef,
  type OnChangeFn,
  type PaginationState,
  type ReactTable,
  type RowData,
  type SortingState,
  columnVisibilityFeature,
  createColumnHelper,
  rowPaginationFeature,
  rowSortingFeature,
  tableFeatures,
  useTable,
} from "@tanstack/react-table";
import { useMemo, useState } from "react";

export const dataTableFeatures = tableFeatures({
  columnVisibilityFeature,
  rowPaginationFeature,
  rowSortingFeature,
});
export type DataTableFeatures = typeof dataTableFeatures;
export type DataTableInstance<TData extends RowData> = ReactTable<DataTableFeatures, TData>;

export const createDataTableColumnHelper = <TData extends RowData>() =>
  createColumnHelper<DataTableFeatures, TData>();

/** The backend's `ListResult<T>`: the paging fields are absent when the list isn't paged. */
export type ListResult<TData> = {
  items: TData[];
  total?: number;
  limit?: number;
  offset?: number;
};

/** A generated sort enum (`"title" | "-title" | …`) minus its descending forms. */
export type SortKey<TSort extends string> = Exclude<TSort, `-${string}`>;

export const PAGE_SIZES = [10, 20, 50, 100] as const;

export type ListParams<TSort extends string> = {
  pagination: PaginationState;
  onPaginationChange: OnChangeFn<PaginationState>;
  sorting: SortingState;
  onSortingChange: OnChangeFn<SortingState>;
  sortable: readonly SortKey<TSort>[];
  /** Spread into the generated `getXOptions({ query })`. */
  query: { limit: number; offset: number; sort?: TSort[] };
};

/**
 * Table state for a server-side list, plus the query it maps to. `TSort` is the
 * endpoint's generated sort enum; `sortable` names the columns that may sort by it,
 * so a key the API doesn't accept is a type error.
 */
export function useListParams<TSort extends string = never>({
  pageSize = 20,
  sortable = [],
}: { pageSize?: number; sortable?: readonly SortKey<TSort>[] } = {}): ListParams<TSort> {
  const [pagination, setPagination] = useState<PaginationState>({ pageIndex: 0, pageSize });
  const [sorting, setSorting] = useState<SortingState>([]);
  // Held in state so an inline array literal stays one reference across renders.
  const [sortableKeys] = useState(sortable);

  const onSortingChange: OnChangeFn<SortingState> = (updater) => {
    setSorting(updater);
    setPagination((p) => ({ ...p, pageIndex: 0 }));
  };

  const sort = sorting.map((s) => (s.desc ? `-${s.id}` : s.id) as TSort);
  return {
    pagination,
    onPaginationChange: setPagination,
    sorting,
    onSortingChange,
    sortable: sortableKeys,
    query: {
      limit: pagination.pageSize,
      offset: pagination.pageIndex * pagination.pageSize,
      ...(sort.length > 0 && { sort }),
    },
  };
}

// A fresh `[]` fallback per render would invalidate the row model every render.
const noRows: never[] = [];

// biome-ignore lint/suspicious/noExplicitAny: column value types differ per column
type DataTableColumns<TData extends RowData> = ColumnDef<DataTableFeatures, TData, any>[];

function columnId<TData extends RowData>(column: DataTableColumns<TData>[number]) {
  return column.id ?? ("accessorKey" in column ? String(column.accessorKey) : undefined);
}

export function useDataTable<TData extends RowData, TSort extends string>({
  columns,
  result,
  params,
  getRowId,
}: {
  columns: DataTableColumns<TData>;
  result: ListResult<TData> | undefined;
  params: ListParams<TSort>;
  getRowId?: (row: TData) => string;
}): DataTableInstance<TData> {
  const { sortable } = params;
  const sortableColumns = useMemo(
    () =>
      columns.map((column) => ({
        ...column,
        enableSorting: (sortable as readonly string[]).includes(columnId(column) ?? ""),
      })),
    [columns, sortable],
  );

  return useTable({
    features: dataTableFeatures,
    columns: sortableColumns,
    data: result?.items ?? noRows,
    rowCount: result?.total,
    manualPagination: true,
    manualSorting: true,
    state: { pagination: params.pagination, sorting: params.sorting },
    onPaginationChange: params.onPaginationChange,
    onSortingChange: params.onSortingChange,
    getRowId,
  });
}
