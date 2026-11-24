import { Page } from "@/components/page";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { type Capability, productName, useCapabilities } from "@/shell/capabilities";
import { Link } from "@tanstack/react-router";

/** The launcher: every app the caller may open, then the platform's tools. */
export function HomePage() {
  const { apps, platform } = useCapabilities();

  return (
    <Page title={productName} description="Choose an app.">
      {apps.length === 0 ? (
        <Empty className="border">
          <EmptyHeader>
            <EmptyTitle>No apps yet</EmptyTitle>
            <EmptyDescription>Each app this suite gains appears here.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <AppGrid label="Apps" apps={apps} />
      )}
      {platform.length > 0 && <AppGrid label="Platform" apps={platform} />}
    </Page>
  );
}

function AppGrid({ label, apps }: { label: string; apps: Capability[] }) {
  return (
    <section aria-label={label} className="space-y-3">
      <h2 className="text-muted-foreground text-sm font-medium">{label}</h2>
      <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 2xl:grid-cols-4">
        {apps.map((app) => (
          <li key={app.id}>
            <Link
              to={app.areas[0].to}
              className="group block rounded-xl focus-visible:outline-none"
            >
              <Card className="group-hover:ring-primary/40 group-focus-visible:ring-ring h-full transition-shadow group-focus-visible:ring-2">
                <CardHeader>
                  <span className="bg-primary/10 text-primary mb-2 flex size-9 items-center justify-center rounded-md">
                    <app.icon className="size-5" />
                  </span>
                  <CardTitle>{app.label}</CardTitle>
                  <CardDescription>{app.description}</CardDescription>
                </CardHeader>
              </Card>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}
