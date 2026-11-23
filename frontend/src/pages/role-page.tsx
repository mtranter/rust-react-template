import { RolePermissions, useManagesAccess } from "@/features/access";
import { Link, getRouteApi } from "@tanstack/react-router";
import { ChevronLeftIcon } from "lucide-react";

const route = getRouteApi("/app/access/roles/$role");

export function RolePage() {
  const { role } = route.useParams();
  const manages = useManagesAccess();

  return (
    <main className="mx-auto max-w-5xl space-y-6 p-6">
      <div className="space-y-2">
        <Link
          to="/access"
          search={{ tab: "roles" }}
          className="text-muted-foreground hover:text-foreground inline-flex items-center gap-1 text-sm"
        >
          <ChevronLeftIcon className="size-4" />
          Roles
        </Link>
        <h1 className="font-heading text-2xl font-semibold">{role}</h1>
      </div>
      {manages === false && (
        <p role="alert" className="text-muted-foreground">
          Only an admin can manage who may do what.
        </p>
      )}
      {manages && <RolePermissions role={role} />}
    </main>
  );
}
