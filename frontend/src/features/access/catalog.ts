import { getAccessCatalogOptions } from "@/api/@tanstack/react-query.gen";
import type { CatalogResourceResponse } from "@/api/types.gen";
import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";

/**
 * The catalog's words for what grants name. A grant whose resource or action its module no longer
 * has (a stale one) isn't in the catalog, so it shows by its name.
 */
export type Labels = {
  resource: (resource: string) => string;
  module: (resource: string) => string | undefined;
  action: (resource: string, action: string) => string;
  scope: (resource: string, scope: string) => string;
};

export function labelsOf(resources: CatalogResourceResponse[]): Labels {
  const find = (resource: string) => resources.find((r) => r.resource === resource);
  return {
    resource: (resource) => find(resource)?.label ?? resource,
    module: (resource) => find(resource)?.moduleLabel,
    action: (resource, action) =>
      find(resource)?.actions.find((a) => a.name === action)?.label ?? action,
    scope: (resource, scope) =>
      find(resource)?.scopes.find((s) => s.name === scope)?.label ?? scope,
  };
}

/** The catalog: each Capability with its Modules' resources, in its order, and every resource. */
export function useCatalog() {
  const catalog = useQuery(getAccessCatalogOptions());
  const data = catalog.data?.capabilities;
  const { capabilities, resources, labels } = useMemo(() => {
    const capabilities = data ?? [];
    const resources = capabilities.flatMap((c) => c.resources);
    return { capabilities, resources, labels: labelsOf(resources) };
  }, [data]);
  return { query: catalog, capabilities, resources, labels };
}
