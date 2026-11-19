import { App } from "@/App";
import { createQueryClient } from "@/lib/query-client";
import type { Session } from "@/lib/sign-in";
import { createAppRouter } from "@/router";
import { createMemoryHistory } from "@tanstack/react-router";
import { render } from "@testing-library/react";

/**
 * Renders the whole app at `path`, its real router included, with the API answered by MSW. With no
 * `session` it signs no one in, as in `make dev` (`src/test/sign-in-fixtures.ts` makes one).
 */
export function renderAt(path: string, { session }: { session?: Session } = {}) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  // Retries off so a failing request fails the test immediately.
  const queryClient = createQueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App router={router} queryClient={queryClient} session={session} />);
  return { router, queryClient };
}
