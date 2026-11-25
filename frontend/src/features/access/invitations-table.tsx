import { getAccessInvitationsOptions } from "@/api/@tanstack/react-query.gen";
import type { InvitationResponse, InvitationStatusParam } from "@/api/types.gen";
import { Badge } from "@/components/ui/badge";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { isErrorBody } from "@/lib/api-errors";
import { useQuery } from "@tanstack/react-query";
import { InvitationActions } from "./invitation-actions";
import { LoadFailed } from "./load-failed";

export const invitationStatuses = ["pending", "accepted", "revoked"] as const;

const statusLabels: Record<InvitationStatusParam, string> = {
  pending: "Pending",
  accepted: "Accepted",
  revoked: "Revoked",
};

/** What the identity provider says of a pending invitee, where the admin may need to act. */
function directoryNote(invitation: InvitationResponse): string | undefined {
  switch (invitation.directoryStatus) {
    case "expired":
      return "Temporary password expired: resend it";
    case "passwordSet":
      return "Password set";
    case "missing":
      return "Deleted at the identity provider: revoke it";
    default:
      return undefined;
  }
}

/**
 * Invitations, newest first, filtered by status. Not paged: the largest page the server answers
 * holds more than an app invites at once.
 */
export function InvitationsTable({
  status,
  onStatus,
}: {
  status?: InvitationStatusParam;
  onStatus: (status: InvitationStatusParam | undefined) => void;
}) {
  const invitations = useQuery(
    getAccessInvitationsOptions({ query: { limit: 100, ...(status ? { status } : {}) } }),
  );

  return (
    <section className="space-y-3" aria-label="Invitations">
      <div className="flex items-center gap-2">
        <label htmlFor="invitation-status" className="text-sm text-muted-foreground">
          Status
        </label>
        <NativeSelect
          id="invitation-status"
          value={status ?? ""}
          onChange={(e) =>
            onStatus(
              invitationStatuses.find((s) => s === e.target.value) as
                | InvitationStatusParam
                | undefined,
            )
          }
        >
          <NativeSelectOption value="">Any</NativeSelectOption>
          {invitationStatuses.map((s) => (
            <NativeSelectOption key={s} value={s}>
              {statusLabels[s]}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </div>
      {invitations.isError ? (
        isErrorBody(invitations.error) && invitations.error.code === "not_configured" ? (
          <p role="alert" className="text-destructive">
            Inviting is off: no identity provider is configured.
          </p>
        ) : (
          <LoadFailed error={invitations.error} what="the invitations" />
        )
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Email</TableHead>
              <TableHead>Name</TableHead>
              <TableHead>Roles</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Invited</TableHead>
              <TableHead>
                <span className="sr-only">Actions</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {invitations.isPending ? (
              <TableRow>
                <TableCell colSpan={6} className="text-muted-foreground">
                  Loading…
                </TableCell>
              </TableRow>
            ) : invitations.data.items.length === 0 ? (
              <TableRow>
                <TableCell colSpan={6} className="text-muted-foreground">
                  No invitations.
                </TableCell>
              </TableRow>
            ) : (
              invitations.data.items.map((invitation) => {
                const note = directoryNote(invitation);
                return (
                  <TableRow key={invitation.id}>
                    <TableCell className="font-medium">{invitation.email}</TableCell>
                    <TableCell>{invitation.name}</TableCell>
                    <TableCell>
                      <ul
                        aria-label={`Roles of ${invitation.email}`}
                        className="flex flex-wrap gap-1"
                      >
                        {invitation.roles.map((role) => (
                          <li key={role}>
                            <Badge variant="secondary">{role}</Badge>
                          </li>
                        ))}
                      </ul>
                    </TableCell>
                    <TableCell>
                      <span className="flex flex-col gap-1">
                        <Badge variant={invitation.status === "pending" ? "outline" : "secondary"}>
                          {statusLabels[invitation.status]}
                        </Badge>
                        {note && <span className="text-xs text-muted-foreground">{note}</span>}
                      </span>
                    </TableCell>
                    <TableCell>
                      <span className="flex flex-col">
                        <time dateTime={invitation.invitedAt}>
                          {new Date(invitation.invitedAt).toLocaleDateString()}
                        </time>
                        {invitation.invitedBy && (
                          <span className="text-xs text-muted-foreground">
                            by {invitation.invitedBy}
                          </span>
                        )}
                      </span>
                    </TableCell>
                    <TableCell className="text-right">
                      {invitation.status === "pending" && (
                        <InvitationActions invitation={invitation} />
                      )}
                    </TableCell>
                  </TableRow>
                );
              })
            )}
          </TableBody>
        </Table>
      )}
    </section>
  );
}
