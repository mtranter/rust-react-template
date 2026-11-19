import { type QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { useState } from "react";
import { Toaster } from "./components/ui/toast";
import { TooltipProvider } from "./components/ui/tooltip";
import { sendAccessToken } from "./hey-api";
import { createQueryClient } from "./lib/query-client";
import { type Session, SessionContext } from "./lib/sign-in";
import { ThemeProvider } from "./lib/theme";
import { type AppRouter, createAppRouter } from "./router";

/**
 * The whole app; tests and the screenshots pass their own router (and query client). `session` is
 * `null` when the app signs no one in, as in `make dev`, where the Vite proxy adds a token.
 */
export function App({
  router,
  queryClient,
  session = null,
}: {
  router?: AppRouter;
  queryClient?: QueryClient;
  session?: Session | null;
}) {
  const [client] = useState(() => queryClient ?? createQueryClient());
  const [appRouter] = useState(() => router ?? createAppRouter());
  // Before the first render, so the first screen's requests carry the token.
  useState(() => sendAccessToken(() => session?.accessToken));
  return (
    <ThemeProvider>
      <SessionContext value={session}>
        <QueryClientProvider client={client}>
          <TooltipProvider>
            <Toaster>
              <RouterProvider router={appRouter} />
            </Toaster>
          </TooltipProvider>
        </QueryClientProvider>
      </SessionContext>
    </ThemeProvider>
  );
}
