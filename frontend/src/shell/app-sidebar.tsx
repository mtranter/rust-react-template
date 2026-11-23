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
import { ChevronsUpDownIcon, LayoutGridIcon } from "lucide-react";
import { type ShellApp, locate, productName, useShellApps } from "./apps";

/**
 * The open app's areas under a switcher to the others; with no app open, the apps themselves.
 * The platform's tools sit in the footer.
 */
export function AppSidebar() {
  const { apps, platform } = useShellApps();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const { app, area } = locate(pathname, [...apps, ...platform]);
  const { setOpenMobile } = useSidebar();
  const close = () => setOpenMobile(false);

  return (
    <Sidebar collapsible="icon">
      <SidebarHeader>
        <AppSwitcher current={app} apps={apps} />
      </SidebarHeader>
      <SidebarContent>
        <nav aria-label={app ? app.label : "Apps"}>
          <SidebarGroup>
            <SidebarGroupLabel>{app ? app.label : "Apps"}</SidebarGroupLabel>
            <SidebarMenu>
              {app
                ? app.areas.map((each) => (
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
                  ))
                : apps.map((each) => (
                    <SidebarMenuItem key={each.path}>
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
      </SidebarContent>
      {platform.length > 0 && (
        <SidebarFooter>
          <nav aria-label="Platform">
            <SidebarMenu>
              {platform.map((each) => (
                <SidebarMenuItem key={each.path}>
                  <SidebarMenuButton
                    isActive={each === app}
                    tooltip={each.label}
                    render={<Link to={each.areas[0].to} onClick={close} />}
                  >
                    <each.icon />
                    <span>{each.label}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </nav>
        </SidebarFooter>
      )}
      <SidebarRail />
    </Sidebar>
  );
}

function AppSwitcher({ current, apps }: { current?: ShellApp; apps: ShellApp[] }) {
  const navigate = useNavigate();
  const { setOpenMobile } = useSidebar();
  const Icon = current?.icon ?? LayoutGridIcon;
  const go = (to: ShellApp["areas"][number]["to"]) => {
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
                    <DropdownMenuItem key={app.path} onClick={() => go(app.areas[0].to)}>
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
