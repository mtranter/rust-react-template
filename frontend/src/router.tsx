import type { GrantSort, NoteSort, NoteSummarySort } from "@/api/types.gen";
import { type ListSearch, validateListSearch } from "@/components/data-table";
import { type GrantFilter, grantSortKeys } from "@/features/access";
import { noteSummarySortKeys } from "@/features/note-search";
import { noteSortKeys } from "@/features/notes";
import { AccessPage } from "@/pages/access-page";
import { AssistantPage } from "@/pages/assistant-page";
import { HomePage } from "@/pages/home-page";
import { RolePage } from "@/pages/role-page";
import { RootLayout } from "@/pages/root-layout";
import { SearchPage } from "@/pages/search-page";
import { SignInCallbackPage } from "@/pages/sign-in-callback-page";
import { SignedOutPage } from "@/pages/signed-out-page";
import {
  type RouterHistory,
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
} from "@tanstack/react-router";

const rootRoute = createRootRoute();

// Pathless: every screen but signing in and out, which a visitor reaches before they are signed in.
const appRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "app",
  component: RootLayout,
});

const callbackRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/auth/callback",
  component: SignInCallbackPage,
});
const signedOutRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/auth/signed-out",
  component: SignedOutPage,
});

const indexRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/",
  validateSearch: (search: Record<string, unknown>): ListSearch<NoteSort> =>
    validateListSearch<NoteSort>(search, noteSortKeys),
  component: HomePage,
});
const homeRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/home",
  beforeLoad: ({ search }) => {
    throw redirect({ to: "/", search });
  },
});

const searchRoute = createRoute({
  getParentRoute: () => appRoute,
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
  getParentRoute: () => appRoute,
  path: "/access",
  validateSearch: (
    search: Record<string, unknown>,
  ): { tab?: AccessTab; search?: string } & GrantFilter & ListSearch<GrantSort> => ({
    tab: accessTabs.find((tab) => tab === search.tab),
    search: typeof search.search === "string" && search.search ? search.search : undefined,
    ...grantFilter(search),
    ...validateListSearch<GrantSort>(search, grantSortKeys),
  }),
  component: AccessPage,
});

const accessTabs = ["subjects", "roles", "grants"] as const;
type AccessTab = (typeof accessTabs)[number];

const roleRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/access/roles/$role",
  component: RolePage,
});

const assistantRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/assistant",
  validateSearch: (search: Record<string, unknown>): { conversation?: string } => ({
    conversation:
      typeof search.conversation === "string" && search.conversation
        ? search.conversation
        : undefined,
  }),
  component: AssistantPage,
});

const routeTree = rootRoute.addChildren([
  appRoute.addChildren([
    indexRoute,
    homeRoute,
    searchRoute,
    accessRoute,
    roleRoute,
    assistantRoute,
  ]),
  callbackRoute,
  signedOutRoute,
]);

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
