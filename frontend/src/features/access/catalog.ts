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

export function isGate(resource: string) {
  return resource.startsWith("module:");
}

/** The catalog's resources, grouped by module in the catalog's order. */
export function byModule(resources: CatalogResourceResponse[]) {
  const modules: { module: string; label: string; resources: CatalogResourceResponse[] }[] = [];
  for (const resource of resources) {
    const group = modules.find((m) => m.module === resource.module);
    if (group) group.resources.push(resource);
    else
      modules.push({
        module: resource.module,
        label: resource.moduleLabel,
        resources: [resource],
      });
  }
  return modules;
}

export function useCatalog() {
  const catalog = useQuery(getAccessCatalogOptions());
  const data = catalog.data?.resources;
  const { resources, labels } = useMemo(() => {
    const resources = data ?? [];
    return { resources, labels: labelsOf(resources) };
  }, [data]);
  return { query: catalog, resources, labels };
}
