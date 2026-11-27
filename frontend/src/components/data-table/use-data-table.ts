import {
  type ColumnDef,
  type OnChangeFn,
  type PaginationState,
  type ReactTable,
  type RowData,
  type SortingState,
  columnVisibilityFeature,
  createColumnHelper,
  functionalUpdate,
  rowPaginationFeature,
  rowSortingFeature,
  tableFeatures,
  useTable,
} from "@tanstack/react-table";
import { useMemo, useState } from "react";
import type { ListPosition } from "./list-search";

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

function sortingOf(sort: string | undefined): SortingState {
  if (sort === undefined) return [];
  return sort.startsWith("-") ? [{ id: sort.slice(1), desc: true }] : [{ id: sort, desc: false }];
}

/**
 * Table state for a server-side list, plus the query it maps to. The caller holds the
 * `page` and `sort` (a page, in its route's search), and hears of a new position through
 * `onChange`. `TSort` is the endpoint's generated sort enum; `sortable` names the columns
 * that may sort by it, so a key the API doesn't accept is a type error.
 */
export function useListParams<TSort extends string = never>({
  page,
  sort,
  onChange,
  pageSize: initialPageSize = 20,
  sortable = [],
}: ListPosition<TSort> & {
  onChange: (position: ListPosition<TSort>) => void;
  pageSize?: number;
  sortable?: readonly SortKey<TSort>[];
}): ListParams<TSort> {
  const [pageSize, setPageSize] = useState(initialPageSize);
  // Held in state so an inline array literal stays one reference across renders.
  const [sortableKeys] = useState(sortable);

  const pagination = useMemo(() => ({ pageIndex: page - 1, pageSize }), [page, pageSize]);
  const sorting = useMemo(() => sortingOf(sort), [sort]);

  const onPaginationChange: OnChangeFn<PaginationState> = (updater) => {
    const next = functionalUpdate(updater, pagination);
    setPageSize(next.pageSize);
    if (next.pageIndex !== pagination.pageIndex) onChange({ page: next.pageIndex + 1, sort });
  };

  const onSortingChange: OnChangeFn<SortingState> = (updater) => {
    const [next] = functionalUpdate(updater, sorting);
    onChange({ page: 1, sort: next && (`${next.desc ? "-" : ""}${next.id}` as TSort) });
  };

  return {
    pagination,
    onPaginationChange,
    sorting,
    onSortingChange,
    sortable: sortableKeys,
    query: {
      limit: pagination.pageSize,
      offset: pagination.pageIndex * pagination.pageSize,
      ...(sort !== undefined && { sort: [sort] }),
    },
  };
}

/** The list has fewer pages than the one asked for: rows went since the link was made. */
export function isPastLastPage<TData extends RowData>(table: DataTableInstance<TData>) {
  const { pageIndex } = table.state.pagination;
  return pageIndex > 0 && pageIndex >= table.getPageCount();
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
  hidden = [],
}: {
  columns: DataTableColumns<TData>;
  result: ListResult<TData> | undefined;
  params: ListParams<TSort>;
  getRowId?: (row: TData) => string;
  /** Column ids hidden until the user shows them from the Columns menu. */
  hidden?: readonly string[];
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
    // The URL holds one sort; shift-click would otherwise ask for a second.
    enableMultiSort: false,
    state: { pagination: params.pagination, sorting: params.sorting },
    onPaginationChange: params.onPaginationChange,
    onSortingChange: params.onSortingChange,
    getRowId,
    initialState: { columnVisibility: Object.fromEntries(hidden.map((id) => [id, false])) },
  });
}
