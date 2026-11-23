import { Button } from "@/components/ui/button";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Kbd } from "@/components/ui/kbd";
import { useAssistantOn } from "@/features/assistant";
import { useNavigate } from "@tanstack/react-router";
import { LayoutGridIcon, SearchIcon, SparklesIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { type Area, useShellApps } from "./apps";

const onMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);

/** Jumps to any area of any app the caller may open: a button in the header, or ⌘K / Ctrl+K anywhere. */
export function GoTo() {
  const [open, setOpen] = useState(false);
  const navigate = useNavigate();
  const { apps, platform } = useShellApps();
  const assistantOn = useAssistantOn();

  useEffect(() => {
    const toggle = (event: KeyboardEvent) => {
      if (event.key === "k" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        setOpen((was) => !was);
      }
    };
    window.addEventListener("keydown", toggle);
    return () => window.removeEventListener("keydown", toggle);
  }, []);

  const go = (to: Area["to"]) => {
    setOpen(false);
    void navigate({ to });
  };

  return (
    <>
      <Button
        variant="outline"
        size="sm"
        aria-label="Go to"
        className="text-muted-foreground md:w-56 md:justify-start"
        onClick={() => setOpen(true)}
      >
        <SearchIcon />
        <span className="hidden md:inline">Go to…</span>
        <Kbd className="ml-auto hidden md:inline-flex">{onMac ? "⌘K" : "Ctrl K"}</Kbd>
      </Button>
      <CommandDialog
        open={open}
        onOpenChange={setOpen}
        title="Go to"
        description="Jump to any app, or any area of one."
      >
        <Command>
          <CommandInput placeholder="Go to…" />
          <CommandList>
            <CommandEmpty>Nothing matches.</CommandEmpty>
            {[...apps, ...platform].map((app) => (
              <CommandGroup key={app.path} heading={app.label}>
                {app.areas.map((area) => (
                  <CommandItem
                    key={area.to}
                    value={`${app.label} ${area.label}`}
                    onSelect={() => go(area.to)}
                  >
                    <area.icon />
                    {area.label}
                  </CommandItem>
                ))}
              </CommandGroup>
            ))}
            <CommandGroup heading="Everywhere">
              {assistantOn && (
                <CommandItem value="Assistant" onSelect={() => go("/assistant")}>
                  <SparklesIcon />
                  Assistant
                </CommandItem>
              )}
              <CommandItem value="All apps" onSelect={() => go("/")}>
                <LayoutGridIcon />
                All apps
              </CommandItem>
            </CommandGroup>
          </CommandList>
        </Command>
      </CommandDialog>
    </>
  );
}
