import { Button, buttonVariants } from "@/components/ui/button";
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet";
import { AssistantChat } from "@/features/assistant";
import { useIsMobile } from "@/hooks/use-mobile";
import { Link } from "@tanstack/react-router";
import { Maximize2Icon, SparklesIcon, XIcon } from "lucide-react";
import { type ReactNode, useState } from "react";

/**
 * The assistant beside whatever app is open, so the user can ask about what they see: docked on the
 * right, or a sheet over the page on a phone. It stays open, on its conversation, across screens.
 */
export function AssistantPanel({ onClose }: { onClose: () => void }) {
  const mobile = useIsMobile();
  const [conversation, setConversation] = useState<string>();

  const toolbar = (title: ReactNode) => (
    <div className="flex h-12 shrink-0 items-center gap-1 border-b px-3">
      <SparklesIcon className="text-primary size-4" />
      {title}
      <Link
        to="/assistant"
        search={{ conversation }}
        aria-label="Open the assistant full page"
        className={`ml-auto ${buttonVariants({ variant: "ghost", size: "icon-sm" })}`}
      >
        <Maximize2Icon />
      </Link>
      <Button variant="ghost" size="icon-sm" aria-label="Close the assistant" onClick={onClose}>
        <XIcon />
      </Button>
    </div>
  );
  const chat = (
    <div className="min-h-0 flex-1 overflow-y-auto p-3">
      <AssistantChat open={conversation} onOpen={setConversation} />
    </div>
  );

  if (mobile) {
    return (
      <Sheet open onOpenChange={(open) => !open && onClose()}>
        <SheetContent
          side="right"
          showCloseButton={false}
          className="gap-0 p-0 data-[side=right]:w-full data-[side=right]:sm:max-w-md"
        >
          {toolbar(<SheetTitle className="text-sm">Assistant</SheetTitle>)}
          {chat}
        </SheetContent>
      </Sheet>
    );
  }
  return (
    <aside
      aria-label="Assistant"
      className="bg-background sticky top-12 flex h-[calc(100svh-3rem)] w-[26rem] shrink-0 flex-col border-l"
    >
      {toolbar(<h2 className="font-heading text-sm font-semibold">Assistant</h2>)}
      {chat}
    </aside>
  );
}
