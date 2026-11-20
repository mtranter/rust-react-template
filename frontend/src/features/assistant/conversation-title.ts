import type { ConversationSummaryResponse } from "@/api/types.gen";

/** A conversation's title, or what to call it until its first message gives it one. */
export function conversationTitle(conversation: Pick<ConversationSummaryResponse, "title">) {
  return conversation.title || "New conversation";
}
