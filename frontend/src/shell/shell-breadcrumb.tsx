import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Link, useRouterState } from "@tanstack/react-router";
import { locate, useShellApps } from "./apps";

/** App › area. On a record inside an area, the area links back to it; the page's heading names the record. */
export function ShellBreadcrumb() {
  const { apps, platform } = useShellApps();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const { app, area } = locate(pathname, [...apps, ...platform]);
  if (!app) return null;

  return (
    <Breadcrumb className="min-w-0">
      <BreadcrumbList className="flex-nowrap">
        <BreadcrumbItem className="hidden md:inline-flex">
          <BreadcrumbLink render={<Link to={app.areas[0].to} />}>{app.label}</BreadcrumbLink>
        </BreadcrumbItem>
        {area && (
          <>
            <BreadcrumbSeparator className="hidden md:block" />
            <BreadcrumbItem className="min-w-0">
              {pathname === area.to ? (
                <BreadcrumbPage className="truncate">{area.label}</BreadcrumbPage>
              ) : (
                <BreadcrumbLink render={<Link to={area.to} />}>{area.label}</BreadcrumbLink>
              )}
            </BreadcrumbItem>
          </>
        )}
      </BreadcrumbList>
    </Breadcrumb>
  );
}
