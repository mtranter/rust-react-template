import { Button } from "@/components/ui/button";
import { ArrowLeftIcon } from "lucide-react";
import { ConversationList } from "./conversation-list";
import { ConversationView } from "./conversation-view";

/**
 * The caller's conversations beside the one `open`, if any. Sized by its container, not the
 * viewport, so it fits the shell's docked panel: narrow, it shows the list or the open one.
 */
export function AssistantChat({
  open,
  onOpen,
}: {
  open?: string;
  onOpen: (id: string | undefined) => void;
}) {
  return (
    <section aria-label="Chat" className="@container">
      <div className="grid gap-6 @3xl:grid-cols-[16rem_1fr]">
        <div className={open ? "hidden @3xl:block" : undefined}>
          <ConversationList open={open} onOpen={onOpen} />
        </div>
        {open ? (
          <div className="min-w-0 space-y-2">
            <Button
              variant="ghost"
              size="sm"
              className="-ml-2 @3xl:hidden"
              onClick={() => onOpen(undefined)}
            >
              <ArrowLeftIcon />
              All conversations
            </Button>
            <ConversationView key={open} id={open} />
          </div>
        ) : (
          <p className="text-muted-foreground hidden @3xl:block">
            Ask the assistant about your data, or to change it for you: start a conversation, or
            open one.
          </p>
        )}
      </div>
    </section>
  );
}
