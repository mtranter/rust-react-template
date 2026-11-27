import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
  useSidebar,
} from "@/components/ui/sidebar";
import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import { ChevronsUpDownIcon, LayoutGridIcon, ShieldCheckIcon } from "lucide-react";
import { type Capability, locate, productName, useCapabilities } from "./capabilities";

/**
 * The open app's areas under a switcher to the others; with no app open, the apps themselves.
 * The platform's tools sit behind one Admin item in the footer, and inside it every tool's areas
 * show together, a group per tool.
 */
export function AppSidebar() {
  const { apps, platform } = useCapabilities();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const { app, area } = locate(pathname, [...apps, ...platform]);
  const { setOpenMobile } = useSidebar();
  const close = () => setOpenMobile(false);
  const inAdmin = app !== undefined && platform.includes(app);
  const areaLinks = (of: Capability) =>
    of.areas.map((each) => (
      <SidebarMenuItem key={each.to}>
        <SidebarMenuButton
          isActive={each === area}
          tooltip={each.label}
          render={<Link to={each.to} onClick={close} />}
        >
          <each.icon />
          <span>{each.label}</span>
        </SidebarMenuButton>
      </SidebarMenuItem>
    ));

  return (
    <Sidebar collapsible="icon">
      <SidebarHeader>
        <AppSwitcher current={app} apps={apps} />
      </SidebarHeader>
      <SidebarContent>
        {inAdmin ? (
          platform.map((tool) => (
            <nav key={tool.id} aria-label={tool.label}>
              <SidebarGroup>
                <SidebarGroupLabel>{tool.label}</SidebarGroupLabel>
                <SidebarMenu>{areaLinks(tool)}</SidebarMenu>
              </SidebarGroup>
            </nav>
          ))
        ) : (
          <nav aria-label={app ? app.label : "Apps"}>
            <SidebarGroup>
              <SidebarGroupLabel>{app ? app.label : "Apps"}</SidebarGroupLabel>
              <SidebarMenu>
                {app
                  ? areaLinks(app)
                  : apps.map((each) => (
                      <SidebarMenuItem key={each.id}>
                        <SidebarMenuButton
                          tooltip={each.label}
                          render={<Link to={each.areas[0].to} onClick={close} />}
                        >
                          <each.icon />
                          <span>{each.label}</span>
                        </SidebarMenuButton>
                      </SidebarMenuItem>
                    ))}
              </SidebarMenu>
            </SidebarGroup>
          </nav>
        )}
      </SidebarContent>
      {platform.length > 0 && (
        <SidebarFooter>
          <nav aria-label="Platform">
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton
                  isActive={inAdmin}
                  tooltip="Admin"
                  render={<Link to={platform[0].areas[0].to} onClick={close} />}
                >
                  <ShieldCheckIcon />
                  <span>Admin</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            </SidebarMenu>
          </nav>
        </SidebarFooter>
      )}
      <SidebarRail />
    </Sidebar>
  );
}

function AppSwitcher({ current, apps }: { current?: Capability; apps: Capability[] }) {
  const navigate = useNavigate();
  const { setOpenMobile } = useSidebar();
  const Icon = current?.icon ?? LayoutGridIcon;
  const go = (to: Capability["areas"][number]["to"]) => {
    setOpenMobile(false);
    void navigate({ to });
  };

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger render={<SidebarMenuButton size="lg" aria-label="Switch app" />}>
            <span className="bg-primary text-primary-foreground flex size-8 shrink-0 items-center justify-center rounded-md">
              <Icon className="size-4" />
            </span>
            <span className="grid flex-1 text-left leading-tight">
              <span className="truncate font-semibold">{current?.label ?? productName}</span>
              <span className="text-muted-foreground truncate text-xs">
                {current ? productName : "All apps"}
              </span>
            </span>
            <ChevronsUpDownIcon className="ml-auto" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="min-w-56">
            {apps.length > 0 && (
              <>
                <DropdownMenuGroup>
                  <DropdownMenuLabel>Apps</DropdownMenuLabel>
                  {apps.map((app) => (
                    <DropdownMenuItem key={app.id} onClick={() => go(app.areas[0].to)}>
                      <app.icon />
                      {app.label}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuGroup>
                <DropdownMenuSeparator />
              </>
            )}
            <DropdownMenuItem onClick={() => go("/")}>
              <LayoutGridIcon />
              All apps
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}
