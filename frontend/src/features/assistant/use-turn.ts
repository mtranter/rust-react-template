import {
  getAssistantConversationsByIdOptions,
  getAssistantConversationsQueryKey,
} from "@/api/@tanstack/react-query.gen";
import type {
  ConversationResponse,
  DecisionRequest,
  ToolCallResponse,
  TurnEventResponse,
} from "@/api/types.gen";
import { changedMeanwhileMessage } from "@/components/changed-meanwhile-alert";
import { errorMessage, isErrorBody, isPreconditionFailed } from "@/lib/api-errors";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { decideToolCall, sendMessage } from "./turn-stream";

/** A tool call as the stream tells it: `running` until its result comes. */
export type LiveToolCall = Omit<ToolCallResponse, "status"> & {
  status: ToolCallResponse["status"] | "running";
};

export type LiveItem = { kind: "text"; text: string } | { kind: "toolCall"; call: LiveToolCall };

/**
 * A turn while it runs: what the caller said, or the stored call they decided on, and what has
 * streamed back so far. `decided` is that call as the stream has told it since.
 */
export type LiveTurn = {
  said?: string;
  decided?: LiveToolCall;
  items: LiveItem[];
  error?: string;
};

const LOST =
  "The connection to the assistant was lost. The turn may still finish: reopen this conversation to see how it ended.";

/**
 * Sends messages to conversation `id`, and decides the tool calls awaiting approval in it, then
 * follows the stream of the turn each runs or resumes. Once a turn ends the conversation is
 * refetched, so its stored messages replace the streamed ones, and when the turn ran a write,
 * every query is invalidated so other screens show what it changed.
 */
export function useTurn(id: string) {
  const queryClient = useQueryClient();
  const [turn, setTurn] = useState<LiveTurn>();
  const [error, setError] = useState<string>();
  const controller = useRef<AbortController>(undefined);
  useEffect(() => () => controller.current?.abort(), []);

  const follow = useCallback(
    async (
      start: LiveTurn,
      events: (signal: AbortSignal) => AsyncGenerator<TurnEventResponse>,
    ): Promise<"ran" | "refused"> => {
      const abort = new AbortController();
      controller.current = abort;
      setError(undefined);
      setTurn(start);
      let live = start;
      let finished: string[] | undefined;
      let failed: string | undefined;
      let refused = false;
      try {
        for await (const event of events(abort.signal)) {
          if (event.kind === "finished") finished = event.messageIds;
          if (event.kind === "error") failed = errorMessage(event.error);
          live = apply(live, event);
          setTurn(live);
        }
      } catch (err) {
        if (abort.signal.aborted) return "ran";
        refused = isErrorBody(err);
        failed = isPreconditionFailed(err)
          ? changedMeanwhileMessage("send", "conversation")
          : refused
            ? errorMessage(err)
            : LOST;
      }
      if (abort.signal.aborted) return "ran";

      const fresh = await queryClient
        .fetchQuery({ ...getAssistantConversationsByIdOptions({ path: { id } }), staleTime: 0 })
        .catch(() => undefined);
      if (abort.signal.aborted) return "ran";
      if ((fresh && finished && ranAWrite(fresh, finished)) || ran(live.decided)) {
        void queryClient.invalidateQueries();
      } else {
        void queryClient.invalidateQueries({ queryKey: getAssistantConversationsQueryKey() });
      }
      setTurn(undefined);
      // A turn that finished stored its error as a message, which the refetch shows.
      if (!finished || !fresh) setError(failed ?? (finished ? undefined : LOST));
      return refused ? "refused" : "ran";
    },
    [id, queryClient],
  );

  const send = useCallback(
    (text: string, version: string) =>
      follow({ said: text, items: [] }, (signal) => sendMessage(id, version, text, signal)),
    [id, follow],
  );

  const decide = useCallback(
    (call: ToolCallResponse, decision: DecisionRequest) =>
      follow({ decided: { ...call, status: "running" }, items: [] }, (signal) =>
        decideToolCall(id, call.id, decision, signal),
      ),
    [id, follow],
  );

  return { turn, error, running: turn !== undefined, send, decide };
}

/** Whether `call`, decided in this turn, was approved and ran without an error. */
function ran(call: LiveToolCall | undefined) {
  return call?.status === "approved" && call.error === undefined;
}

function apply(turn: LiveTurn, event: TurnEventResponse): LiveTurn {
  const items = [...turn.items];
  const last = items.at(-1);
  const update = (callId: string, change: (call: LiveToolCall) => LiveToolCall) => {
    const at = items.findIndex((item) => item.kind === "toolCall" && item.call.id === callId);
    const item = items[at];
    if (item?.kind === "toolCall") items[at] = { kind: "toolCall", call: change(item.call) };
    return at >= 0;
  };
  switch (event.kind) {
    case "text":
      if (last?.kind === "text")
        items[items.length - 1] = { kind: "text", text: last.text + event.text };
      else items.push({ kind: "text", text: event.text });
      return { ...turn, items };
    case "toolCallStarted":
      items.push({
        kind: "toolCall",
        call: {
          id: event.id,
          tool: event.tool,
          summary: event.summary,
          arguments: event.arguments,
          status: "running",
        },
      });
      return { ...turn, items };
    case "toolResult":
      if (turn.decided?.id === event.id) {
        return { ...turn, decided: { ...turn.decided, result: event.result, error: event.error } };
      }
      update(event.id, (call) => ({
        ...call,
        status: "ran",
        result: event.result,
        error: event.error,
      }));
      return { ...turn, items };
    case "decided":
      if (turn.decided?.id !== event.id) return turn;
      return { ...turn, decided: { ...turn.decided, status: event.status } };
    case "approvalRequired": {
      const awaiting = (call: LiveToolCall): LiveToolCall => ({
        ...call,
        summary: event.summary || call.summary,
        proposal: event.proposal,
        status: "awaiting_approval",
      });
      if (!update(event.id, awaiting)) {
        items.push({
          kind: "toolCall",
          call: awaiting({
            id: event.id,
            tool: event.tool,
            summary: event.summary,
            arguments: event.arguments,
            status: "running",
          }),
        });
      }
      return { ...turn, items };
    }
    case "error":
      return { ...turn, error: errorMessage(event.error) };
    default:
      return turn;
  }
}

/** Whether the messages `stored` by a turn include a write the caller approved and that ran. */
function ranAWrite(conversation: ConversationResponse, stored: string[]) {
  return conversation.messages.some(
    (message) =>
      stored.includes(message.id) &&
      message.kind === "toolCalls" &&
      message.toolCalls.some((call) => call.status === "approved" && call.error === undefined),
  );
}
