import { AssistantChat, useAssistantOn } from "@/features/assistant";

export function AssistantPage() {
  const on = useAssistantOn();

  return (
    <main className="mx-auto max-w-4xl space-y-6 p-6">
      <h1 className="font-heading text-2xl font-semibold">Assistant</h1>
      {on === false && (
        <p role="alert" className="text-muted-foreground">
          The assistant isn't available.
        </p>
      )}
      {on && <AssistantChat />}
    </main>
  );
}
