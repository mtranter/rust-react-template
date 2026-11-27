import { client } from "@/api/client.gen";
import { publicFetch } from "@/hey-api";
import { queryOptions, useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { applyTheme } from "./apply-theme";
import type { ThemeTokens } from "./theme-css";

export type Theme = ThemeTokens & {
  version: number;
  updatedAt?: string;
  updatedBy?: string;
};

export type ThemeFetcher = () => Promise<Theme | null>;

/** At the generated client's base URL; `null` keeps the defaults. */
export const fetchTheme: ThemeFetcher = async () => {
  const { baseUrl = "/api" } = client.getConfig();
  const response = await publicFetch(new Request(`${baseUrl}/ui-settings/theme`));
  if (!response.ok) return null;
  return (await response.json()) as Theme;
};

export function themeQueryOptions(fetcher: ThemeFetcher = fetchTheme) {
  return queryOptions({
    queryKey: ["ui-settings", "theme"] as const,
    queryFn: () => fetcher().catch(() => null),
    refetchOnWindowFocus: true,
    refetchInterval: 5 * 60_000,
    retry: false,
  });
}

/** Keeps the org's theme applied; a failed fetch leaves the cached one (or the defaults) alone. */
export function ThemeLoader({ fetcher }: { fetcher?: ThemeFetcher }) {
  const { data } = useQuery(themeQueryOptions(fetcher));
  useEffect(() => {
    if (data)
      applyTheme({ light: data.light ?? {}, dark: data.dark ?? {}, shared: data.shared ?? {} });
  }, [data]);
  return null;
}
