import type { ConnectionStatus } from "@/api/types.gen";
import { Badge } from "@/components/ui/badge";

const shown: Record<
  ConnectionStatus,
  { label: string; variant: "secondary" | "outline" | "default" | "destructive" }
> = {
  not_configured: { label: "Not configured", variant: "outline" },
  untested: { label: "Untested", variant: "secondary" },
  ok: { label: "OK", variant: "default" },
  failing: { label: "Failing", variant: "destructive" },
};

/** A Connection's status, as **Test connection** last found it since it last changed. */
export function ConnectionStatusBadge({ status }: { status: ConnectionStatus }) {
  const { label, variant } = shown[status];
  return <Badge variant={variant}>{label}</Badge>;
}
