import { getAssistantStatusOptions } from "@/api/@tanstack/react-query.gen";
import { useQuery } from "@tanstack/react-query";

/**
 * Whether the assistant is on for the caller: the app has a model configured, and they may chat (a
 * `403` is off too). `undefined` until the answer comes.
 */
export function useAssistantOn(): boolean | undefined {
  const status = useQuery({ ...getAssistantStatusOptions(), retry: false, staleTime: 60_000 });
  if (status.isPending) return undefined;
  return status.data?.on === true;
}
