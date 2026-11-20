import { getAssistantConversationsByIdOptions } from "@/api/@tanstack/react-query.gen";
import type { MessageResponse, ToolCallResponse } from "@/api/types.gen";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { isErrorBody } from "@/lib/api-errors";
import { useQuery } from "@tanstack/react-query";
import { ChevronRightIcon, SendIcon } from "lucide-react";
import { type FormEvent, type KeyboardEvent, useState } from "react";
import { conversationTitle } from "./conversation-title";
import { Markdown } from "./markdown";
import { type LiveItem, type LiveToolCall, type LiveTurn, useTurn } from "./use-turn";

const STATUS: Record<LiveToolCall["status"], string> = {
  running: "Running",
  ran: "Ran",
  awaiting_approval: "Awaiting approval",
  approved: "Approved",
  rejected: "Rejected",
};

/** One conversation's messages, oldest first, then the turn running in it and a box to write in. */
export function ConversationView({ id }: { id: string }) {
  const conversation = useQuery(getAssistantConversationsByIdOptions({ path: { id } }));
  const { turn, error, running, send } = useTurn(id);

  if (conversation.isPending) return null;
  if (conversation.isError) {
    const gone = isErrorBody(conversation.error) && conversation.error.code === "not_found";
    return (
      <p role="alert" className="text-destructive">
        {gone ? "This conversation isn't there any more." : "Failed to load the conversation."}
      </p>
    );
  }

  const { messages, version } = conversation.data;
  const title = conversationTitle(conversation.data);
  return (
    <article aria-label={title} className="min-w-0 space-y-4">
      <h2 className="font-heading text-lg font-semibold">{title}</h2>
      {messages.length === 0 && !turn ? (
        <p className="text-muted-foreground">No messages yet.</p>
      ) : (
        <ol className="space-y-3">
          {messages.map((message) => (
            <li key={message.id}>
              <MessageItem message={message} />
            </li>
          ))}
          {turn && <LiveTurnItems turn={turn} />}
        </ol>
      )}
      {error && (
        <p role="alert" className="text-destructive text-sm">
          {error}
        </p>
      )}
      <Composer running={running} send={(text) => send(text, version)} />
    </article>
  );
}

function Composer({
  running,
  send,
}: {
  running: boolean;
  send: (text: string) => Promise<"ran" | "refused">;
}) {
  const [text, setText] = useState("");
  const submit = async (event?: FormEvent) => {
    event?.preventDefault();
    const said = text.trim();
    if (!said || running) return;
    setText("");
    if ((await send(said)) === "refused") setText((now) => now || said);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing)
      void submit(event);
  };
  return (
    <form onSubmit={submit} className="flex items-end gap-2">
      <Textarea
        aria-label="Message"
        placeholder="Ask the assistant…"
        value={text}
        disabled={running}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={onKeyDown}
      />
      <Button type="submit" disabled={running || !text.trim()}>
        <SendIcon />
        Send
      </Button>
    </form>
  );
}

function LiveTurnItems({ turn }: { turn: LiveTurn }) {
  return (
    <>
      <li>
        <UserText text={turn.said} />
      </li>
      {turn.items.map((item, at) => (
        // The stream only appends, or changes an item in place, so its position is stable.
        // biome-ignore lint/suspicious/noArrayIndexKey: see above
        <li key={at}>
          <LiveItemView item={item} />
        </li>
      ))}
      {turn.error ? (
        <li role="alert" className="text-destructive text-sm">
          {turn.error}
        </li>
      ) : (
        <li className="flex items-center gap-2 text-muted-foreground text-sm">
          <Spinner aria-label="Working" />
          The assistant is working…
        </li>
      )}
    </>
  );
}

function LiveItemView({ item }: { item: LiveItem }) {
  return item.kind === "text" ? (
    <AssistantText text={item.text} />
  ) : (
    <ToolCallRow call={item.call} />
  );
}

function MessageItem({ message }: { message: MessageResponse }) {
  switch (message.kind) {
    case "user":
      return <UserText text={message.text} />;
    case "assistant":
      return <AssistantText text={message.text} />;
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
    case "error":
      return (
        <p aria-label="Error" className="text-destructive text-sm">
          {message.text}
        </p>
      );
    default:
      return null;
  }
}

function UserText({ text }: { text: string }) {
  return (
    <div className="flex justify-end">
      <p
        aria-label="You"
        className="max-w-[80%] rounded-lg bg-primary px-3 py-2 whitespace-pre-wrap text-primary-foreground"
      >
        {text}
      </p>
    </div>
  );
}

function AssistantText({ text }: { text: string }) {
  return (
    <div aria-label="Assistant" className="max-w-[80%]">
      <Markdown text={text} />
    </div>
  );
}

function ToolCallRow({ call }: { call: ToolCallResponse | LiveToolCall }) {
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
