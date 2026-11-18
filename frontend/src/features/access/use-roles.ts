import { getAccessRolesOptions } from "@/api/@tanstack/react-query.gen";
import { useQuery } from "@tanstack/react-query";

/** Every role, by name: few enough that the largest page the server answers holds them all. */
export function useRoles() {
  return useQuery(getAccessRolesOptions({ query: { limit: 100 } }));
}
