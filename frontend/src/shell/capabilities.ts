import { useMyPermissions } from "@/features/access";
import { useHiddenApps } from "@/features/ui-settings";
import type { LinkProps } from "@tanstack/react-router";
import {
  EyeIcon,
  HistoryIcon,
  KeyRoundIcon,
  ListChecksIcon,
  ListIcon,
  type LucideIcon,
  MailIcon,
  NotebookPenIcon,
  PaletteIcon,
  PlugIcon,
  SearchIcon,
  ShieldIcon,
  UsersIcon,
} from "lucide-react";

/** Names the whole suite where no one app is open: the launcher's heading, the switcher. */
export const productName = "rust-vite-template";

/**
 * One page of an app. It is shown only to a caller who holds `needs`, an action named as its
 * Module's policies name it (`make check-modules` fails on one no Module declares). Hiding is a
 * convenience: the server still refuses the operations themselves.
 */
export type Area = {
  label: string;
  to: NonNullable<LinkProps["to"]>;
  icon: LucideIcon;
  needs: { resource: string; action: string };
};

/**
 * One Capability of the suite (ADR 0047) and its app: every route under `path` is one of its
 * `areas` or a record in one. `id` is its `capability.yaml`'s, which `make check-modules` reads
 * from here as a literal `id: "<id>"`.
 */
export type Capability = {
  id: string;
  label: string;
  description: string;
  icon: LucideIcon;
  path: string;
  /** In the sidebar's order; the first is where the app opens. */
  areas: [Area, ...Area[]];
};

const notes: Capability = {
  id: "notes",
  label: "Notes",
  description: "Write notes, keep their checklists and find them again.",
  icon: NotebookPenIcon,
  path: "/notes",
  areas: [
    {
      label: "All notes",
      to: "/notes",
      icon: ListIcon,
      needs: { resource: "note", action: "list" },
    },
    {
      label: "Search",
      to: "/notes/search",
      icon: SearchIcon,
      needs: { resource: "noteSummary", action: "search" },
    },
  ],
};

const access: Capability = {
  id: "access",
  label: "Access",
  description: "Who may use the apps, and what each role may do.",
  icon: ShieldIcon,
  path: "/access",
  areas: [
    {
      label: "Users",
      to: "/access/subjects",
      icon: UsersIcon,
      needs: { resource: "subject", action: "list" },
    },
    {
      label: "Roles",
      to: "/access/roles",
      icon: KeyRoundIcon,
      needs: { resource: "role", action: "list" },
    },
    {
      label: "Invitations",
      to: "/access/invitations",
      icon: MailIcon,
      needs: { resource: "invitation", action: "list" },
    },
    {
      label: "Permissions",
      to: "/access/permissions",
      icon: ListChecksIcon,
      needs: { resource: "grant", action: "list" },
    },
    {
      label: "Audit log",
      to: "/access/audit",
      icon: HistoryIcon,
      needs: { resource: "auditEntry", action: "list" },
    },
  ],
};

const integrations: Capability = {
  id: "connections",
  label: "Integrations",
  description: "The other systems the apps call: where each is, and its credentials.",
  icon: PlugIcon,
  path: "/integrations",
  areas: [
    {
      label: "Integrations",
      to: "/integrations",
      icon: PlugIcon,
      needs: { resource: "connection", action: "list" },
    },
  ],
};

const uiSettings: Capability = {
  id: "ui-settings",
  label: "UI settings",
  description: "How the app looks, and which apps everyone sees.",
  icon: PaletteIcon,
  path: "/ui-settings",
  areas: [
    {
      label: "Theming",
      to: "/ui-settings/theming",
      icon: PaletteIcon,
      needs: { resource: "theme", action: "update" },
    },
    {
      label: "App visibility",
      to: "/ui-settings/app-visibility",
      icon: EyeIcon,
      needs: { resource: "appVisibility", action: "update" },
    },
  ],
};

/** Every app, and the platform's tools, kept apart in the sidebar's footer, in the switcher's order. */
export const registry: { apps: Capability[]; platform: Capability[] } = {
  apps: [notes],
  platform: [access, integrations, uiSettings],
};

/**
 * The apps the caller may open, each with only the areas whose `needs` they hold, leaving out an
 * app with none and any the organisation hides (ADR 0052: the platform's tools can't be hidden,
 * so no admin hides their own way back); nothing until the server has answered both.
 */
export function useCapabilities(): { apps: Capability[]; platform: Capability[] } {
  const hidden = useHiddenApps();
  const answered = useMyPermissions();
  const permissions = hidden ? (answered ?? []) : [];
  const held = (area: Area) =>
    permissions.some((p) => p.resource === area.needs.resource && p.action === area.needs.action);
  const usable = (capabilities: Capability[]) =>
    capabilities.flatMap((capability) => {
      const [first, ...rest] = capability.areas.filter(held);
      return first ? [{ ...capability, areas: [first, ...rest] as Capability["areas"] }] : [];
    });
  const shown = registry.apps.filter((app) => !hidden?.includes(app.id));
  return { apps: usable(shown), platform: usable(registry.platform) };
}

/**
 * An app's own hue, for its icon's tile only: spread by the golden angle over the registry's order,
 * so a dozen apps stay apart and adding one never recolours the rest.
 */
export function appTint(app: Capability): { backgroundColor: string; color: string } {
  const index = [...registry.apps, ...registry.platform].findIndex((each) => each.id === app.id);
  const hue = (250 + Math.max(index, 0) * 137.508) % 360;
  return {
    backgroundColor: `oklch(0.93 0.05 ${hue})`,
    color: `oklch(0.42 0.14 ${hue})`,
  };
}

/** The app whose `path` holds `pathname`, and its area: the one whose `to` holds it most closely. */
export function locate(pathname: string, apps: Capability[]): { app?: Capability; area?: Area } {
  const holds = (path: string) => pathname === path || pathname.startsWith(`${path}/`);
  const app = apps.find((candidate) => holds(candidate.path));
  const area = app?.areas
    .filter((candidate) => holds(candidate.to))
    .sort((a, b) => b.to.length - a.to.length)[0];
  return { app, area };
}
