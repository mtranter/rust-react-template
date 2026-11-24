import { getAccessMePermissionsOptions } from "@/api/@tanstack/react-query.gen";
import type { PermissionResponse } from "@/api/types.gen";
import { useQuery } from "@tanstack/react-query";

/**
 * What the caller may do on some row: each resource and action, as the server's rules in force
 * answer it. The SPA reads no roles from its token, so it asks. `undefined` until the answer
 * comes, and on a failure, so nothing shows rather than everything.
 */
export function useMyPermissions(): PermissionResponse[] | undefined {
  const permissions = useQuery({
    ...getAccessMePermissionsOptions(),
    retry: false,
    staleTime: 60_000,
  });
  return permissions.data?.permissions;
}

/** Whether the caller may perform `action` on `resource`; `undefined` until known. */
export function usePermitted(resource: string, action: string): boolean | undefined {
  const permissions = useMyPermissions();
  return permissions?.some((p) => p.resource === resource && p.action === action);
}
