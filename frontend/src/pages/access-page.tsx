import { listSearch } from "@/components/data-table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
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

const route = getRouteApi("/access");

export function AccessPage() {
  const { tab, page = 1, sort, role, resource, action, search } = route.useSearch();
  const navigate = route.useNavigate();
  const manages = useManagesAccess();
  const current = tab ?? "grants";

  return (
    <main className="mx-auto max-w-5xl space-y-6 p-6">
      <div className="flex items-baseline justify-between gap-4">
        <h1 className="font-heading text-2xl font-semibold">Access</h1>
        {manages &&
          (current === "roles" ? (
            <CreateRoleDialog />
          ) : current === "subjects" ? (
            <AssignRoleDialog />
          ) : (
            <GrantAccessDialog />
          ))}
      </div>
      {manages === false && (
        <p role="alert" className="text-muted-foreground">
          Only an admin can manage who may do what.
        </p>
      )}
      {manages && (
        <Tabs
          value={current}
          onValueChange={(value) =>
            navigate({
              search: (prev) => ({
                ...prev,
                tab: value === "roles" || value === "subjects" ? value : undefined,
              }),
            })
          }
        >
          <TabsList>
            <TabsTrigger value="grants">Grants</TabsTrigger>
            <TabsTrigger value="roles">Roles</TabsTrigger>
            <TabsTrigger value="subjects">Users</TabsTrigger>
          </TabsList>
          <TabsContent value="grants">
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
          </TabsContent>
          <TabsContent value="roles">
            <RolesTable />
          </TabsContent>
          <TabsContent value="subjects">
            <SubjectsTable
              search={search}
              onSearch={(text) => navigate({ search: (prev) => ({ ...prev, search: text }) })}
            />
          </TabsContent>
        </Tabs>
      )}
    </main>
  );
}
