import type { Header, RowData } from "@tanstack/react-table";
import { ArrowDownIcon, ArrowUpIcon, ChevronsUpDownIcon } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "../ui/button";
import { Skeleton } from "../ui/skeleton";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../ui/table";
import type { DataTableFeatures, DataTableInstance } from "./use-data-table";

const ariaSort = { asc: "ascending", desc: "descending", none: "none" } as const;

function SortableHeader<TData extends RowData>({
  table,
  header,
}: {
  table: DataTableInstance<TData>;
  header: Header<DataTableFeatures, TData, unknown>;
}) {
  const sorted = header.column.getIsSorted();
  const Icon =
    sorted === "asc" ? ArrowUpIcon : sorted === "desc" ? ArrowDownIcon : ChevronsUpDownIcon;
  return (
    <Button
      variant="ghost"
      size="sm"
      className="-ml-2.5"
      onClick={header.column.getToggleSortingHandler()}
    >
      <table.FlexRender header={header} />
      <Icon className={sorted ? undefined : "text-muted-foreground"} />
    </Button>
  );
}

const skeletonRows = Array.from({ length: 5 }, (_, i) => `skeleton-${i}`);

export function DataTable<TData extends RowData>({
  table,
  isLoading = false,
  empty = "No results.",
}: {
  table: DataTableInstance<TData>;
  isLoading?: boolean;
  empty?: ReactNode;
}) {
  const columns = table.getVisibleLeafColumns();
  const rows = table.getRowModel().rows;

  return (
    <div className="overflow-hidden rounded-lg border">
      <Table>
        <TableHeader>
          {table.getHeaderGroups().map((group) => (
            <TableRow key={group.id}>
              {group.headers.map((header) => (
                <TableHead
                  key={header.id}
                  colSpan={header.colSpan}
                  aria-sort={
                    header.column.getCanSort()
                      ? ariaSort[header.column.getIsSorted() || "none"]
                      : undefined
                  }
                >
                  {header.isPlaceholder ? null : header.column.getCanSort() ? (
                    <SortableHeader table={table} header={header} />
                  ) : (
                    <table.FlexRender header={header} />
                  )}
                </TableHead>
              ))}
            </TableRow>
          ))}
        </TableHeader>
        <TableBody aria-busy={isLoading}>
          {isLoading && rows.length === 0 ? (
            skeletonRows.map((id) => (
              <TableRow key={id}>
                {columns.map((column) => (
                  <TableCell key={column.id}>
                    <Skeleton className="h-4 w-full" />
                  </TableCell>
                ))}
              </TableRow>
            ))
          ) : rows.length > 0 ? (
            rows.map((row) => (
              <TableRow key={row.id}>
                {row.getVisibleCells().map((cell) => (
                  <TableCell key={cell.id}>
                    <table.FlexRender cell={cell} />
                  </TableCell>
                ))}
              </TableRow>
            ))
          ) : (
            <TableRow>
              <TableCell
                colSpan={columns.length}
                className="h-24 text-center text-muted-foreground"
              >
                {empty}
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
    </div>
  );
}
