import type { GrantSort, NoteSort, NoteSummarySort } from "@/api/types.gen";
import { type ListSearch, validateListSearch } from "@/components/data-table";
import { type GrantFilter, grantSortKeys } from "@/features/access";
import { noteSummarySortKeys } from "@/features/note-search";
import { noteSortKeys } from "@/features/notes";
import { AccessPage } from "@/pages/access-page";
import { HomePage } from "@/pages/home-page";
import { RootLayout } from "@/pages/root-layout";
import { SearchPage } from "@/pages/search-page";
import {
  type RouterHistory,
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
} from "@tanstack/react-router";

const rootRoute = createRootRoute({ component: RootLayout });

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  validateSearch: (search: Record<string, unknown>): ListSearch<NoteSort> =>
    validateListSearch<NoteSort>(search, noteSortKeys),
  component: HomePage,
});
const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/home",
  beforeLoad: ({ search }) => {
    throw redirect({ to: "/", search });
  },
});

const searchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/search",
  validateSearch: (
    search: Record<string, unknown>,
  ): { q?: string } & ListSearch<NoteSummarySort> => ({
    q: typeof search.q === "string" && search.q ? search.q : undefined,
    ...validateListSearch<NoteSummarySort>(search, noteSummarySortKeys),
  }),
  component: SearchPage,
});

const accessRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/access",
  validateSearch: (search: Record<string, unknown>): GrantFilter & ListSearch<GrantSort> => ({
    ...grantFilter(search),
    ...validateListSearch<GrantSort>(search, grantSortKeys),
  }),
  component: AccessPage,
});

const routeTree = rootRoute.addChildren([indexRoute, homeRoute, searchRoute, accessRoute]);

function grantFilter(search: Record<string, unknown>): GrantFilter {
  const text = (value: unknown) => (typeof value === "string" && value ? value : undefined);
  return { role: text(search.role), resource: text(search.resource), action: text(search.action) };
}

export function createAppRouter(history?: RouterHistory) {
  return createRouter({ routeTree, history });
}

export type AppRouter = ReturnType<typeof createAppRouter>;

declare module "@tanstack/react-router" {
  interface Register {
    router: AppRouter;
  }
}
