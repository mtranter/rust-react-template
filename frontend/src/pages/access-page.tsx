import { listSearch } from "@/components/data-table";
import { Page } from "@/components/page";
import {
  AssignRoleDialog,
  CreateRoleDialog,
  GrantAccessDialog,
  GrantsTable,
  RolesTable,
  SubjectsTable,
  useManagesAccess,
} from "@/features/access";
import { getRouteApi } from "@tanstack/react-router";

const usersRoute = getRouteApi("/app/access/subjects");
const permissionsRoute = getRouteApi("/app/access/permissions");

export function NotAnAdmin() {
  return (
    <p role="alert" className="text-muted-foreground">
      Only an admin can manage who may do what.
    </p>
  );
}

export function AccessUsersPage() {
  const { search } = usersRoute.useSearch();
  const navigate = usersRoute.useNavigate();
  const manages = useManagesAccess();

  return (
    <Page
      title="Users"
      description="Everyone who has signed in or been given a role, with their roles."
      actions={manages && <AssignRoleDialog />}
    >
      {manages === false && <NotAnAdmin />}
      {manages && (
        <SubjectsTable
          search={search}
          onSearch={(text) => navigate({ search: (prev) => ({ ...prev, search: text }) })}
        />
      )}
    </Page>
  );
}

export function AccessRolesPage() {
  const manages = useManagesAccess();

  return (
    <Page
      title="Roles"
      description="Each role, and what it is for. Open one to choose what it may do."
      actions={manages && <CreateRoleDialog />}
    >
      {manages === false && <NotAnAdmin />}
      {manages && <RolesTable />}
    </Page>
  );
}

export function AccessPermissionsPage() {
  const { page = 1, sort, role, resource, action } = permissionsRoute.useSearch();
  const navigate = permissionsRoute.useNavigate();
  const manages = useManagesAccess();

  return (
    <Page
      title="Permissions"
      description="Every grant, by role, resource and action."
      actions={manages && <GrantAccessDialog />}
    >
      {manages === false && <NotAnAdmin />}
      {manages && (
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
      )}
    </Page>
  );
}
