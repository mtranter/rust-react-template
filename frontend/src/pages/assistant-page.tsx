import { AssistantChat, useAssistantOn } from "@/features/assistant";
import { getRouteApi } from "@tanstack/react-router";

const route = getRouteApi("/app/assistant");

export function AssistantPage() {
  const on = useAssistantOn();
  const { conversation } = route.useSearch();
  const navigate = route.useNavigate();

  return (
    <main className="mx-auto max-w-5xl space-y-6 p-6">
      <h1 className="font-heading text-2xl font-semibold">Assistant</h1>
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
    </main>
  );
}
