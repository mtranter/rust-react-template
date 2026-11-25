import type { GrantSort, NoteSort, NoteSummarySort } from "@/api/types.gen";
import { type ListSearch, validateListSearch } from "@/components/data-table";
import { type GrantFilter, grantSortKeys, invitationStatuses } from "@/features/access";
import { noteSummarySortKeys } from "@/features/note-search";
import { noteSortKeys } from "@/features/notes";
import {
  AccessInvitationsPage,
  AccessPermissionsPage,
  AccessRolesPage,
  AccessUsersPage,
} from "@/pages/access-page";
import { AssistantPage } from "@/pages/assistant-page";
import { HomePage } from "@/pages/home-page";
import { NotesPage } from "@/pages/notes-page";
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
  component: HomePage,
});

const notesRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/notes",
});
const notesIndexRoute = createRoute({
  getParentRoute: () => notesRoute,
  path: "/",
  validateSearch: (search: Record<string, unknown>): ListSearch<NoteSort> =>
    validateListSearch<NoteSort>(search, noteSortKeys),
  component: NotesPage,
});
const notesSearchRoute = createRoute({
  getParentRoute: () => notesRoute,
  path: "/search",
  validateSearch: (
    search: Record<string, unknown>,
  ): { q?: string } & ListSearch<NoteSummarySort> => ({
    q: typeof search.q === "string" && search.q ? search.q : undefined,
    ...validateListSearch<NoteSummarySort>(search, noteSummarySortKeys),
  }),
  component: SearchPage,
});
// Where the notes list and its search were before the suite had apps.
const homeRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/home",
  beforeLoad: ({ search }) => {
    throw redirect({ to: "/notes", search });
  },
});
const searchRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/search",
  beforeLoad: ({ search }) => {
    throw redirect({ to: "/notes/search", search });
  },
});

// `?tab=` is where the access screen's areas were when they were tabs of one page.
const accessRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/access",
  beforeLoad: ({ search }) => {
    const { tab, ...rest } = search as Record<string, unknown>;
    throw redirect({
      to:
        tab === "roles"
          ? "/access/roles"
          : tab === "grants"
            ? "/access/permissions"
            : tab === "invitations"
              ? "/access/invitations"
              : "/access/subjects",
      search: rest,
    });
  },
});
const accessUsersRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/access/subjects",
  validateSearch: (search: Record<string, unknown>): { search?: string } => ({
    search: typeof search.search === "string" && search.search ? search.search : undefined,
  }),
  component: AccessUsersPage,
});
const accessRolesRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/access/roles",
  component: AccessRolesPage,
});
const accessInvitationsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/access/invitations",
  validateSearch: (
    search: Record<string, unknown>,
  ): { status?: (typeof invitationStatuses)[number] } => ({
    status: invitationStatuses.find((s) => s === search.status),
  }),
  component: AccessInvitationsPage,
});
const accessPermissionsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/access/permissions",
  validateSearch: (search: Record<string, unknown>): GrantFilter & ListSearch<GrantSort> => ({
    ...grantFilter(search),
    ...validateListSearch<GrantSort>(search, grantSortKeys),
  }),
  component: AccessPermissionsPage,
});

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
    notesRoute.addChildren([notesIndexRoute, notesSearchRoute]),
    homeRoute,
    searchRoute,
    accessRoute,
    accessUsersRoute,
    accessRolesRoute,
    accessInvitationsRoute,
    accessPermissionsRoute,
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
