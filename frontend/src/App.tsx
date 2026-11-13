import { type QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { useState } from "react";
import { Toaster } from "./components/ui/toast";
import { TooltipProvider } from "./components/ui/tooltip";
import { createQueryClient } from "./lib/query-client";
import { ThemeProvider } from "./lib/theme";
import { type AppRouter, createAppRouter } from "./router";

/** The whole app; tests and the screenshots pass their own router (and query client). */
export function App({ router, queryClient }: { router?: AppRouter; queryClient?: QueryClient }) {
  const [client] = useState(() => queryClient ?? createQueryClient());
  const [appRouter] = useState(() => router ?? createAppRouter());
  return (
    <ThemeProvider>
      <QueryClientProvider client={client}>
        <TooltipProvider>
          <Toaster>
            <RouterProvider router={appRouter} />
          </Toaster>
        </TooltipProvider>
      </QueryClientProvider>
    </ThemeProvider>
  );
}
