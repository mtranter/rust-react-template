import { isForbidden } from "@/lib/api-errors";

/** A read that failed: refused, because the caller may not manage this, or failed outright. */
export function LoadFailed({ error, what }: { error: unknown; what: string }) {
  return (
    <p role="alert" className={isForbidden(error) ? "text-muted-foreground" : "text-destructive"}>
      {isForbidden(error)
        ? "You may not manage this: ask an admin for access."
        : `Failed to load ${what}.`}
    </p>
  );
}
