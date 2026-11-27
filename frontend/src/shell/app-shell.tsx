import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { useAssistantOn } from "@/features/assistant";
import { Outlet, useRouterState } from "@tanstack/react-router";
import { SparklesIcon } from "lucide-react";
import { useState } from "react";
import { AppSidebar } from "./app-sidebar";
import { AssistantPanel } from "./assistant-panel";
import { locate, useCapabilities } from "./capabilities";
import { GoTo } from "./go-to";
import { ShellBreadcrumb } from "./shell-breadcrumb";
import { UserMenu } from "./user-menu";

const panelKey = "assistant-panel";

/** Every screen inside the suite: the sidebar, a header over the page, and the assistant beside it. */
export function AppShell() {
  const assistantOn = useAssistantOn();
  const onAssistantPage = useRouterState({
    select: (state) => state.location.pathname === "/assistant",
  });
  const [panelOpen, setPanelOpen] = useState(() => remembered(panelKey));
  const toggle = (open: boolean) => {
    remember(panelKey, open);
    setPanelOpen(open);
  };
  const panel = assistantOn === true && !onAssistantPage;
  const { apps, platform } = useCapabilities();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const { app } = locate(pathname, [...apps, ...platform]);

  return (
    <SidebarProvider data-app={app?.id ?? "home"}>
      <AppSidebar />
      <div className="bg-background relative flex min-w-0 flex-1 flex-col">
        <header className="bg-background/95 sticky top-0 z-20 flex h-12 shrink-0 items-center gap-2 border-b border-t-4 border-t-primary px-3 backdrop-blur">
          <SidebarTrigger />
          <Separator
            orientation="vertical"
            className="mr-1 data-vertical:h-4 data-vertical:self-center"
          />
          <ShellBreadcrumb />
          <div className="ml-auto flex items-center gap-1">
            <GoTo />
            {panel && (
              <Button
                variant={panelOpen ? "secondary" : "ghost"}
                size="icon-sm"
                aria-label="Assistant"
                aria-pressed={panelOpen}
                onClick={() => toggle(!panelOpen)}
              >
                <SparklesIcon />
              </Button>
            )}
            <ThemeToggle />
            <UserMenu />
          </div>
        </header>
        <div className="flex flex-1">
          <main className="min-w-0 flex-1">
            <Outlet />
          </main>
          {panel && panelOpen && <AssistantPanel onClose={() => toggle(false)} />}
        </div>
      </div>
    </SidebarProvider>
  );
}

function remembered(key: string) {
  try {
    return localStorage.getItem(key) === "open";
  } catch {
    return false;
  }
}

function remember(key: string, open: boolean) {
  try {
    localStorage.setItem(key, open ? "open" : "closed");
  } catch {
    // Not remembered, but still open for this visit.
  }
}
