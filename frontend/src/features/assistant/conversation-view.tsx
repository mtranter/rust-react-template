import { getAssistantConversationsByIdOptions } from "@/api/@tanstack/react-query.gen";
import type { DecisionRequest, MessageResponse, ToolCallResponse } from "@/api/types.gen";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { isErrorBody } from "@/lib/api-errors";
import { useQuery } from "@tanstack/react-query";
import { CheckIcon, ChevronRightIcon, SendIcon, XIcon } from "lucide-react";
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
  const { turn, error, running, send, decide } = useTurn(id);

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
              <MessageItem
                message={message}
                decided={turn?.decided}
                decide={running ? undefined : decide}
              />
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
      {turn.said !== undefined && (
        <li>
          <UserText text={turn.said} />
        </li>
      )}
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
  if (item.kind === "text") return <AssistantText text={item.text} />;
  // Decided once the turn has ended and the stored call replaces this one.
  return <ToolCall call={item.call} />;
}

type Decide = (call: ToolCallResponse, decision: DecisionRequest) => Promise<unknown>;

/**
 * `message` as stored, but for the call `decided` in the turn running, shown as the stream tells
 * it. `decide` is absent while a turn runs.
 */
function MessageItem({
  message,
  decided,
  decide,
}: {
  message: MessageResponse;
  decided?: LiveToolCall;
  decide?: Decide;
}) {
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
              {decided?.id === call.id ? (
                <ToolCall call={decided} />
              ) : (
                <ToolCall call={call} decide={decide && ((decision) => decide(call, decision))} />
              )}
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

/** A pending write as a card to approve or reject; any other call as a row. */
function ToolCall({
  call,
  decide,
}: {
  call: ToolCallResponse | LiveToolCall;
  decide?: (decision: DecisionRequest) => void;
}) {
  return call.status === "awaiting_approval" ? (
    <PendingWrite call={call} decide={decide} />
  ) : (
    <ToolCallRow call={call} />
  );
}

/**
 * A write the assistant asked to make as the caller, waiting for them: what it does, and with
 * what. Its buttons are disabled while a turn runs, `decide` absent.
 */
function PendingWrite({
  call,
  decide,
}: {
  call: ToolCallResponse | LiveToolCall;
  decide?: (decision: DecisionRequest) => void;
}) {
  const title = call.summary || call.tool;
  return (
    <section aria-label={`Approve ${call.tool}?`}>
      <Card size="sm">
        <CardHeader>
          <CardTitle>{title}</CardTitle>
          <CardDescription>
            The assistant wants to run <code className="font-mono">{call.tool}</code> as you.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Json label="Arguments" value={call.arguments} />
        </CardContent>
        <CardFooter className="gap-2">
          <Button size="sm" disabled={!decide} onClick={() => decide?.("approve")}>
            <CheckIcon />
            Approve
          </Button>
          <Button size="sm" variant="outline" disabled={!decide} onClick={() => decide?.("reject")}>
            <XIcon />
            Reject
          </Button>
        </CardFooter>
      </Card>
    </section>
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
