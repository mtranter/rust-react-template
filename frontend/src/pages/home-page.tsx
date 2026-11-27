import { Page } from "@/components/page";
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
      <h2 className="font-heading text-lg font-semibold">{label}</h2>
      <ul className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 2xl:grid-cols-4">
        {apps.map((app) => (
          <li key={app.id} data-app={app.id}>
            <Link
              to={app.areas[0].to}
              className="group bg-primary text-primary-foreground focus-visible:ring-ring relative flex h-full min-h-44 flex-col justify-end overflow-hidden rounded-2xl p-5 transition-transform focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none hover:-translate-y-0.5 motion-reduce:transition-none motion-reduce:hover:translate-y-0"
            >
              <app.icon
                aria-hidden
                strokeWidth={1.25}
                className="absolute -top-4 -right-4 size-32 opacity-20 transition-transform group-hover:rotate-[-8deg] motion-reduce:transition-none"
              />
              <span className="font-heading text-2xl font-bold tracking-tight">{app.label}</span>
              <span className="mt-1 max-w-[30ch] text-sm opacity-85">{app.description}</span>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}
