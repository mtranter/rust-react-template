import { getAccessCatalogOptions } from "@/api/@tanstack/react-query.gen";
import { useQuery } from "@tanstack/react-query";

/**
 * Whether the caller may manage access: the catalog answers only those who may. The SPA reads no
 * roles from its token (in dev it holds none: the proxy adds it), so it asks.
 * `undefined` until the answer comes.
 */
export function useManagesAccess(): boolean | undefined {
  const catalog = useQuery({ ...getAccessCatalogOptions(), retry: false, staleTime: 60_000 });
  if (catalog.isPending) return undefined;
  return catalog.isSuccess;
}
