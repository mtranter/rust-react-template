import { getConnectionsByKeyOptions } from "@/api/@tanstack/react-query.gen";
import { Page } from "@/components/page";
import { ConnectionForm, ConnectionsTable } from "@/features/connections";
import { useQuery } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";

const connectionRoute = getRouteApi("/app/integrations/$key");

export function IntegrationsPage() {
  return (
    <Page
      title="Integrations"
      description="The other systems the apps call, and how each is connected."
    >
      <ConnectionsTable />
    </Page>
  );
}

export function IntegrationPage() {
  const { key } = connectionRoute.useParams();
  const connection = useQuery(getConnectionsByKeyOptions({ path: { key } }));

  return (
    <Page
      title={connection.data?.label ?? key}
      description="Where it is, how the apps sign in to it, and whether it answers."
      width="narrow"
    >
      <ConnectionForm connectionKey={key} />
    </Page>
  );
}
