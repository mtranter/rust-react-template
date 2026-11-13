import type { SortKey } from "./use-data-table";

/** A list's place in its route's search; the defaults (page 1, no sort) are left out. */
export type ListSearch<TSort extends string> = { page?: number; sort?: TSort };

/** Which rows a server-paged list shows: `page` is 1-based, `sort` as the API takes it. */
export type ListPosition<TSort extends string> = { page: number; sort?: TSort };

/**
 * What of a route's search is a valid `ListSearch`, so a stale or malformed link falls back to
 * the defaults. An invalid value comes back as `undefined` because the router lays this over
 * the raw search: a key left out would keep the bad value.
 */
export function validateListSearch<TSort extends string>(
  { page, sort }: Record<string, unknown>,
  sortable: readonly SortKey<TSort>[],
): ListSearch<TSort> {
  const isSort =
    typeof sort === "string" && (sortable as readonly string[]).includes(sort.replace(/^-/, ""));
  return {
    page: Number.isSafeInteger(page) && (page as number) > 1 ? (page as number) : undefined,
    sort: isSort ? (sort as TSort) : undefined,
  };
}

/**
 * A new position as search params. `page` is present, as `undefined`, even on page 1, so that
 * spread over the previous search it clears the old page.
 */
export function listSearch<TSort extends string>({
  page,
  sort,
}: ListPosition<TSort>): ListSearch<TSort> {
  return { page: page > 1 ? page : undefined, sort };
}
