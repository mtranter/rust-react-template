import { getConnectionsOptions } from "@/api/@tanstack/react-query.gen";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { isForbidden } from "@/lib/api-errors";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ConnectionStatusBadge } from "./connection-status-badge";

/** Every Connection the app's modules declare, by key, with its status. Not paged: an app has few. */
export function ConnectionsTable() {
  const connections = useQuery(getConnectionsOptions({ query: { limit: 100 } }));

  return (
    <section className="space-y-3" aria-label="Integrations">
      {connections.isError ? (
        <p
          role="alert"
          className={isForbidden(connections.error) ? "text-muted-foreground" : "text-destructive"}
        >
          {isForbidden(connections.error)
            ? "You may not manage integrations: ask an admin for access."
            : "Failed to load the integrations."}
        </p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>System</TableHead>
              <TableHead>URL</TableHead>
              <TableHead>Status</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {connections.isPending ? (
              <TableRow>
                <TableCell colSpan={3} className="text-muted-foreground">
                  Loading…
                </TableCell>
              </TableRow>
            ) : connections.data.items.length === 0 ? (
              <TableRow>
                <TableCell colSpan={3} className="text-muted-foreground">
                  No module calls another system yet.
                </TableCell>
              </TableRow>
            ) : (
              connections.data.items.map((connection) => (
                <TableRow key={connection.key}>
                  <TableCell className="font-medium">
                    <Link
                      to="/integrations/$key"
                      params={{ key: connection.key }}
                      className="underline-offset-4 hover:underline"
                    >
                      {connection.label}
                    </Link>
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {connection.url ?? "Not set"}
                  </TableCell>
                  <TableCell>
                    <ConnectionStatusBadge status={connection.status} />
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      )}
    </section>
  );
}
