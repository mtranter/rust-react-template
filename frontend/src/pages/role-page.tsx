import { Page } from "@/components/page";
import { RolePermissions } from "@/features/access";
import { getRouteApi } from "@tanstack/react-router";

const route = getRouteApi("/app/access/roles/$role");

export function RolePage() {
  const { role } = route.useParams();

  return (
    <Page title={role} description="What this role may do, app by app.">
      <RolePermissions role={role} />
    </Page>
  );
}
