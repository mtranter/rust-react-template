import { toast } from "@/components/ui/toast";
import { MutationCache, QueryClient, type QueryClientConfig } from "@tanstack/react-query";
import { errorMessage, shownByForm } from "./api-errors";

declare module "@tanstack/react-query" {
  interface Register {
    mutationMeta: {
      /** The inputs of the form running this mutation: field errors naming them aren't toasted. */
      formFields?: readonly string[];
    };
  }
}

/** Toasts every failed mutation, except one whose errors its form shows beside the inputs. */
export function createQueryClient(config: QueryClientConfig = {}) {
  return new QueryClient({
    ...config,
    mutationCache: new MutationCache({
      onError: (error, _variables, _context, mutation) => {
        if (shownByForm(error, mutation.meta?.formFields)) return;
        toast.add({ type: "error", title: "Couldn't save", description: errorMessage(error) });
      },
    }),
  });
}
