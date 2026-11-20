import { ConversationList } from "./conversation-list";
import { ConversationView } from "./conversation-view";

/** The caller's conversations beside the one `open`, if any. */
export function AssistantChat({
  open,
  onOpen,
}: {
  open?: string;
  onOpen: (id: string | undefined) => void;
}) {
  return (
    <section aria-label="Chat" className="grid gap-6 md:grid-cols-[16rem_1fr]">
      <ConversationList open={open} onOpen={onOpen} />
      {open ? (
        <ConversationView key={open} id={open} />
      ) : (
        <p className="text-muted-foreground">
          Ask the assistant about your data, or to change it for you: start a conversation, or open
          one.
        </p>
      )}
    </section>
  );
}
