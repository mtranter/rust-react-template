import type { ConnectionStatus } from "@/api/types.gen";
import { Badge } from "@/components/ui/badge";
import type { ComponentProps } from "react";

const shown: Record<
  ConnectionStatus,
  { label: string; variant: ComponentProps<typeof Badge>["variant"] }
> = {
  not_configured: { label: "Not configured", variant: "outline" },
  untested: { label: "Untested", variant: "secondary" },
  ok: { label: "OK", variant: "success" },
  failing: { label: "Failing", variant: "destructive" },
};

/** A Connection's status, as **Test connection** last found it since it last changed. */
export function ConnectionStatusBadge({ status }: { status: ConnectionStatus }) {
  const { label, variant } = shown[status];
  return <Badge variant={variant}>{label}</Badge>;
}
