import { useManagesAccess } from "@/features/access";
import type { LinkProps } from "@tanstack/react-router";
import {
  KeyRoundIcon,
  ListChecksIcon,
  ListIcon,
  type LucideIcon,
  NotebookPenIcon,
  SearchIcon,
  ShieldIcon,
  UsersIcon,
} from "lucide-react";

/** Names the whole suite where no one app is open: the launcher's heading, the switcher. */
export const productName = "rust-vite-template";

export type Area = { label: string; to: NonNullable<LinkProps["to"]>; icon: LucideIcon };

/** One app of the suite: every route under `path` is one of its `areas` or a record in one. */
export type ShellApp = {
  label: string;
  description: string;
  icon: LucideIcon;
  path: string;
  /** In the sidebar's order; the first is where the app opens. */
  areas: [Area, ...Area[]];
};

const notes: ShellApp = {
  label: "Notes",
  description: "Write notes, keep their checklists and find them again.",
  icon: NotebookPenIcon,
  path: "/notes",
  areas: [
    { label: "All notes", to: "/notes", icon: ListIcon },
    { label: "Search", to: "/notes/search", icon: SearchIcon },
  ],
};

const access: ShellApp = {
  label: "Access",
  description: "Who may use the apps, and what each role may do.",
  icon: ShieldIcon,
  path: "/access",
  areas: [
    { label: "Users", to: "/access/subjects", icon: UsersIcon },
    { label: "Roles", to: "/access/roles", icon: KeyRoundIcon },
    { label: "Permissions", to: "/access/permissions", icon: ListChecksIcon },
  ],
};

/**
 * The apps the caller may open, in the switcher's order, and the platform's tools, kept apart in
 * the sidebar's footer. An app the caller may not use is left out, as `access` is from a caller
 * who may not manage access.
 */
export function useShellApps(): { apps: ShellApp[]; platform: ShellApp[] } {
  const managesAccess = useManagesAccess();
  return { apps: [notes], platform: managesAccess ? [access] : [] };
}

/** The app whose `path` holds `pathname`, and its area: the one whose `to` holds it most closely. */
export function locate(pathname: string, apps: ShellApp[]): { app?: ShellApp; area?: Area } {
  const holds = (path: string) => pathname === path || pathname.startsWith(`${path}/`);
  const app = apps.find((candidate) => holds(candidate.path));
  const area = app?.areas
    .filter((candidate) => holds(candidate.to))
    .sort((a, b) => b.to.length - a.to.length)[0];
  return { app, area };
}
