import { Page } from "@/components/page";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { type Capability, appTint, productName, useCapabilities } from "@/shell/capabilities";
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
      <h2 className="font-heading text-lg font-semibold">{label}</h2>
      <ul className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 2xl:grid-cols-4">
        {apps.map((app) => (
          <li key={app.id}>
            <Link
              to={app.areas[0].to}
              className="group bg-card hover:border-primary focus-visible:ring-ring flex h-full min-h-40 flex-col gap-4 rounded-2xl border-2 p-5 transition-colors focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none motion-reduce:transition-none"
            >
              <span
                style={appTint(app)}
                className="flex size-12 items-center justify-center rounded-xl"
              >
                <app.icon aria-hidden className="size-6" />
              </span>
              <span className="mt-auto">
                <span className="font-heading block text-2xl font-bold tracking-tight">
                  {app.label}
                </span>
                <span className="text-muted-foreground mt-1 block max-w-[34ch] text-sm">
                  {app.description}
                </span>
              </span>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}
