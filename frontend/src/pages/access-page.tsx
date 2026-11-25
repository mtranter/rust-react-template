import { listSearch } from "@/components/data-table";
import { Page } from "@/components/page";
import {
  AssignRoleDialog,
  CreateRoleDialog,
  GrantAccessDialog,
  GrantsTable,
  InvitationsTable,
  InviteUserDialog,
  RolesTable,
  SubjectsTable,
  usePermitted,
} from "@/features/access";
import { getRouteApi } from "@tanstack/react-router";

const usersRoute = getRouteApi("/app/access/subjects");
const invitationsRoute = getRouteApi("/app/access/invitations");
const permissionsRoute = getRouteApi("/app/access/permissions");

export function AccessUsersPage() {
  const { search } = usersRoute.useSearch();
  const navigate = usersRoute.useNavigate();
  const assigns = usePermitted("assignment", "assign");

  return (
    <Page
      title="Users"
      description="Everyone who has signed in or been given a role, with their roles."
      actions={assigns && <AssignRoleDialog />}
    >
      <SubjectsTable
        search={search}
        onSearch={(text) => navigate({ search: (prev) => ({ ...prev, search: text }) })}
      />
    </Page>
  );
}

export function AccessRolesPage() {
  const creates = usePermitted("role", "create");

  return (
    <Page
      title="Roles"
      description="Each role, and what it is for. Open one to choose what it may do."
      actions={creates && <CreateRoleDialog />}
    >
      <RolesTable />
    </Page>
  );
}

export function AccessInvitationsPage() {
  const { status } = invitationsRoute.useSearch();
  const navigate = invitationsRoute.useNavigate();
  const invites = usePermitted("invitation", "invite");

  return (
    <Page
      title="Invitations"
      description="People invited by email, and whether they have signed in yet."
      actions={invites && <InviteUserDialog />}
    >
      <InvitationsTable
        status={status}
        onStatus={(next) => navigate({ search: (prev) => ({ ...prev, status: next }) })}
      />
    </Page>
  );
}

export function AccessPermissionsPage() {
  const { page = 1, sort, role, resource, action } = permissionsRoute.useSearch();
  const navigate = permissionsRoute.useNavigate();
  const grants = usePermitted("grant", "create");

  return (
    <Page
      title="Permissions"
      description="Every grant, by role, resource and action."
      actions={grants && <GrantAccessDialog />}
    >
      <GrantsTable
        page={page}
        sort={sort}
        filter={{ role, resource, action }}
        onChange={(position) =>
          navigate({ search: (prev) => ({ ...prev, ...listSearch(position) }) })
        }
        onFilter={(filter) =>
          navigate({ search: (prev) => ({ ...prev, ...filter, page: undefined }) })
        }
      />
    </Page>
  );
}
