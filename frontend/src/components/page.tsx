import { cn } from "cn";
import type { ReactNode } from "react";

/**
 * A screen's frame inside the shell: its heading, what it is for, its primary actions on the
 * right, then its content. `wide` (the default) suits lists and tables; `narrow` a form or a record.
 */
export function Page({
  title,
  description,
  actions,
  width = "wide",
  children,
}: {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  width?: "wide" | "narrow";
  children?: ReactNode;
}) {
  return (
    <div
      className={cn(
        "mx-auto w-full space-y-6 p-4 md:p-6",
        width === "wide" ? "max-w-screen-2xl" : "max-w-3xl",
      )}
    >
      <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
        <div className="min-w-0 space-y-1">
          <h1 className="font-heading text-3xl font-bold tracking-tight [font-stretch:90%] md:text-4xl">
            {title}
          </h1>
          {description && <p className="text-muted-foreground text-sm">{description}</p>}
        </div>
        {actions && <div className="flex items-center gap-2">{actions}</div>}
      </div>
      {children}
    </div>
  );
}
