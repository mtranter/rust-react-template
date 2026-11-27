import { getAppearanceAppsOptions } from "@/api/@tanstack/react-query.gen";
import { useQuery } from "@tanstack/react-query";

export const hiddenAppsQueryOptions = () => ({
  ...getAppearanceAppsOptions(),
  retry: false,
  staleTime: 60_000,
});

/**
 * The ids of the apps the organisation hides: `undefined` until the server answers, so nothing
 * shows rather than a hidden app flashing past; none on a failure, as hiding is only a
 * convenience (ADR 0052).
 */
export function useHiddenApps(): string[] | undefined {
  const visibility = useQuery(hiddenAppsQueryOptions());
  if (visibility.isError) return [];
  return visibility.data?.hidden;
}
