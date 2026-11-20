import {
  getAssistantConversationsByIdOptions,
  getAssistantConversationsQueryKey,
} from "@/api/@tanstack/react-query.gen";
import type { ConversationResponse, ToolCallResponse, TurnEventResponse } from "@/api/types.gen";
import { changedMeanwhileMessage } from "@/components/changed-meanwhile-alert";
import { errorMessage, isErrorBody, isPreconditionFailed } from "@/lib/api-errors";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { sendMessage } from "./turn-stream";

/** A tool call as the stream tells it: `running` until its result comes. */
export type LiveToolCall = Omit<ToolCallResponse, "status"> & {
  status: ToolCallResponse["status"] | "running";
};

export type LiveItem = { kind: "text"; text: string } | { kind: "toolCall"; call: LiveToolCall };

/** A turn while it runs: what the caller said, and what has streamed back so far. */
export type LiveTurn = { said: string; items: LiveItem[]; error?: string };

const LOST =
  "The connection to the assistant was lost. The turn may still finish: reopen this conversation to see how it ended.";

/**
 * Sends messages to conversation `id` and follows each turn's stream. Once a turn ends the
 * conversation is refetched, so its stored messages replace the streamed ones, and when the turn
 * ran a write, every query is invalidated so other screens show what it changed.
 */
export function useTurn(id: string) {
  const queryClient = useQueryClient();
  const [turn, setTurn] = useState<LiveTurn>();
  const [error, setError] = useState<string>();
  const controller = useRef<AbortController>(undefined);
  useEffect(() => () => controller.current?.abort(), []);

  const send = useCallback(
    async (text: string, version: string): Promise<"ran" | "refused"> => {
      const abort = new AbortController();
      controller.current = abort;
      setError(undefined);
      setTurn({ said: text, items: [] });
      let finished: string[] | undefined;
      let failed: string | undefined;
      let refused = false;
      try {
        for await (const event of sendMessage(id, version, text, abort.signal)) {
          if (event.kind === "finished") finished = event.messageIds;
          if (event.kind === "error") failed = errorMessage(event.error);
          setTurn((live) => live && apply(live, event));
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
      if (fresh && finished && ranAWrite(fresh, finished)) {
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

  return { turn, error, running: turn !== undefined, send };
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
        call: { id: event.id, tool: event.tool, arguments: event.arguments, status: "running" },
      });
      return { ...turn, items };
    case "toolResult":
      update(event.id, (call) => ({
        ...call,
        status: "ran",
        result: event.result,
        error: event.error,
      }));
      return { ...turn, items };
    case "approvalRequired": {
      const awaiting = (call: LiveToolCall): LiveToolCall => ({
        ...call,
        status: "awaiting_approval",
      });
      if (!update(event.id, awaiting)) {
        items.push({
          kind: "toolCall",
          call: awaiting({
            id: event.id,
            tool: event.tool,
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
