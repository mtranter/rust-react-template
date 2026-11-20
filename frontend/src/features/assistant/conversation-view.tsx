import { getAssistantConversationsByIdOptions } from "@/api/@tanstack/react-query.gen";
import type { MessageResponse, ToolCallResponse } from "@/api/types.gen";
import { Badge } from "@/components/ui/badge";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { isErrorBody } from "@/lib/api-errors";
import { useQuery } from "@tanstack/react-query";
import { ChevronRightIcon } from "lucide-react";
import { conversationTitle } from "./conversation-title";

const STATUS: Record<ToolCallResponse["status"], string> = {
  ran: "Ran",
  awaiting_approval: "Awaiting approval",
  approved: "Approved",
  rejected: "Rejected",
};

/** One conversation's messages, oldest first. */
export function ConversationView({ id }: { id: string }) {
  const conversation = useQuery(getAssistantConversationsByIdOptions({ path: { id } }));

  if (conversation.isPending) return null;
  if (conversation.isError) {
    const gone = isErrorBody(conversation.error) && conversation.error.code === "not_found";
    return (
      <p role="alert" className="text-destructive">
        {gone ? "This conversation isn't there any more." : "Failed to load the conversation."}
      </p>
    );
  }

  const title = conversationTitle(conversation.data);
  return (
    <article aria-label={title} className="min-w-0 space-y-4">
      <h2 className="font-heading text-lg font-semibold">{title}</h2>
      {conversation.data.messages.length === 0 ? (
        <p className="text-muted-foreground">No messages yet.</p>
      ) : (
        <ol className="space-y-3">
          {conversation.data.messages.map((message) => (
            <li key={message.id}>
              <MessageItem message={message} />
            </li>
          ))}
        </ol>
      )}
    </article>
  );
}

function MessageItem({ message }: { message: MessageResponse }) {
  switch (message.kind) {
    case "user":
      return (
        <div className="flex justify-end">
          <p
            aria-label="You"
            className="max-w-[80%] rounded-lg bg-primary px-3 py-2 whitespace-pre-wrap text-primary-foreground"
          >
            {message.text}
          </p>
        </div>
      );
    case "assistant":
      return (
        <p aria-label="Assistant" className="max-w-[80%] whitespace-pre-wrap">
          {message.text}
        </p>
      );
    case "toolCalls":
      return (
        <ul className="space-y-1">
          {message.toolCalls.map((call) => (
            <li key={call.id}>
              <ToolCallRow call={call} />
            </li>
          ))}
        </ul>
      );
    default:
      return null;
  }
}

function ToolCallRow({ call }: { call: ToolCallResponse }) {
  return (
    <Collapsible className="rounded-md border text-sm">
      <CollapsibleTrigger className="group flex w-full items-center gap-2 px-3 py-2 text-left">
        <ChevronRightIcon className="size-4 transition-transform group-data-[panel-open]:rotate-90" />
        <code className="font-mono">{call.tool}</code>
        <Badge variant={call.error ? "destructive" : "secondary"}>{STATUS[call.status]}</Badge>
      </CollapsibleTrigger>
      <CollapsibleContent className="space-y-2 border-t px-3 py-2">
        <Json label="Arguments" value={call.arguments} />
        {call.result !== undefined && <Json label="Result" value={call.result} />}
        {call.error !== undefined && <Json label="Error" value={call.error} />}
      </CollapsibleContent>
    </Collapsible>
  );
}

function Json({ label, value }: { label: string; value: unknown }) {
  return (
    <figure className="space-y-1">
      <figcaption className="text-muted-foreground text-xs">{label}</figcaption>
      <pre className="overflow-x-auto rounded bg-muted p-2 text-xs">
        {JSON.stringify(value, null, 2)}
      </pre>
    </figure>
  );
}
