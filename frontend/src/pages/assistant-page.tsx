import { Page } from "@/components/page";
import { AssistantChat, useAssistantOn } from "@/features/assistant";
import { getRouteApi } from "@tanstack/react-router";

const route = getRouteApi("/app/assistant");

export function AssistantPage() {
  const on = useAssistantOn();
  const { conversation } = route.useSearch();
  const navigate = route.useNavigate();

  return (
    <Page title="Assistant">
      {on === false && (
        <p role="alert" className="text-muted-foreground">
          The assistant isn't available.
        </p>
      )}
      {on && (
        <AssistantChat
          open={conversation}
          onOpen={(id) => navigate({ search: (prev) => ({ ...prev, conversation: id }) })}
        />
      )}
    </Page>
  );
}
