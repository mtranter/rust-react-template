import { Page } from "@/components/page";
import { RolePermissions, useManagesAccess } from "@/features/access";
import { getRouteApi } from "@tanstack/react-router";
import { NotAnAdmin } from "./access-page";

const route = getRouteApi("/app/access/roles/$role");

export function RolePage() {
  const { role } = route.useParams();
  const manages = useManagesAccess();

  return (
    <Page title={role} description="What this role may do, app by app.">
      {manages === false && <NotAnAdmin />}
      {manages && <RolePermissions role={role} />}
    </Page>
  );
}
