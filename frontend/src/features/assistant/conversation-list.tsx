import {
  deleteAssistantConversationsByIdMutation,
  getAssistantConversationsByIdQueryKey,
  getAssistantConversationsOptions,
  getAssistantConversationsQueryKey,
  postAssistantConversationsMutation,
} from "@/api/@tanstack/react-query.gen";
import type { ConversationSummaryResponse } from "@/api/types.gen";
import { ChangedMeanwhileAlert } from "@/components/changed-meanwhile-alert";
import { Button } from "@/components/ui/button";
import { isPreconditionFailed } from "@/lib/api-errors";
import { ifMatch } from "@/lib/if-match";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { PlusIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { conversationTitle } from "./conversation-title";

/** As many as the server pages at most: the list doesn't page yet. */
const LIMIT = 100;

/** The caller's conversations, newest first (the server's order), to open, start and delete. */
export function ConversationList({
  open,
  onOpen,
}: {
  open?: string;
  onOpen: (id: string | undefined) => void;
}) {
  const queryClient = useQueryClient();
  const listKey = getAssistantConversationsQueryKey();
  const conversations = useQuery(getAssistantConversationsOptions({ query: { limit: LIMIT } }));
  const [changedMeanwhile, setChangedMeanwhile] = useState(false);

  const start = useMutation({
    ...postAssistantConversationsMutation(),
    onSuccess: (started) => {
      void queryClient.invalidateQueries({ queryKey: listKey });
      onOpen(started.id);
    },
  });

  const remove = useMutation({
    ...deleteAssistantConversationsByIdMutation(),
    meta: { reports: ["precondition_failed"] },
    onMutate: () => setChangedMeanwhile(false),
    onSuccess: (_, { path }) => {
      queryClient.removeQueries({
        queryKey: getAssistantConversationsByIdQueryKey({ path }),
      });
      void queryClient.invalidateQueries({ queryKey: listKey });
      if (path.id === open) onOpen(undefined);
    },
    onError: (error) => {
      if (!isPreconditionFailed(error)) return;
      void queryClient.invalidateQueries({ queryKey: listKey });
      setChangedMeanwhile(true);
    },
  });

  const deleteConversation = (conversation: ConversationSummaryResponse) =>
    remove.mutate({ path: { id: conversation.id }, headers: ifMatch(conversation.version) });

  return (
    <nav aria-label="Conversations" className="space-y-3">
      <Button className="w-full" onClick={() => start.mutate({})} disabled={start.isPending}>
        <PlusIcon />
        New conversation
      </Button>
      {changedMeanwhile && <ChangedMeanwhileAlert retry="delete" subject="conversation" />}
      {conversations.isError ? (
        <p role="alert" className="text-destructive text-sm">
          Failed to load conversations.
        </p>
      ) : conversations.isPending ? null : conversations.data.items.length === 0 ? (
        <p className="text-muted-foreground text-sm">No conversations yet.</p>
      ) : (
        <ul className="space-y-1">
          {conversations.data.items.map((conversation) => {
            const title = conversationTitle(conversation);
            const current = conversation.id === open;
            return (
              <li key={conversation.id} className="flex items-center gap-1">
                <Button
                  variant="ghost"
                  aria-current={current ? "page" : undefined}
                  className={`h-auto min-w-0 flex-1 flex-col items-start gap-0 py-2 text-left ${current ? "bg-muted" : ""}`}
                  onClick={() => onOpen(conversation.id)}
                >
                  <span className="w-full truncate">{title}</span>
                  <span className="text-muted-foreground text-xs font-normal">
                    {new Date(conversation.createdAt).toLocaleString()}
                  </span>
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={`Delete ${title}`}
                  disabled={remove.isPending}
                  onClick={() => deleteConversation(conversation)}
                >
                  <Trash2Icon />
                </Button>
              </li>
            );
          })}
        </ul>
      )}
    </nav>
  );
}
