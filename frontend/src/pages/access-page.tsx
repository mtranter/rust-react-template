import { listSearch } from "@/components/data-table";
import { GrantAccessDialog, GrantsTable, useManagesAccess } from "@/features/access";
import { getRouteApi } from "@tanstack/react-router";

const route = getRouteApi("/access");

export function AccessPage() {
  const { page = 1, sort, role, resource, action } = route.useSearch();
  const navigate = route.useNavigate();
  const manages = useManagesAccess();

  return (
    <main className="mx-auto max-w-5xl space-y-6 p-6">
      <div className="flex items-baseline justify-between gap-4">
        <h1 className="font-heading text-2xl font-semibold">Access</h1>
        {manages && <GrantAccessDialog />}
      </div>
      {manages === false && (
        <p role="alert" className="text-muted-foreground">
          Only an admin can manage who may do what.
        </p>
      )}
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
    </main>
  );
}
