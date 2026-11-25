import {
  deleteAccessInvitationsByIdMutation,
  getAccessInvitationsQueryKey,
  getAccessSubjectsQueryKey,
  postAccessInvitationsByIdResendMutation,
} from "@/api/@tanstack/react-query.gen";
import type { InvitationResponse } from "@/api/types.gen";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import { errorMessage, isErrorBody, isPreconditionFailed } from "@/lib/api-errors";
import { ifMatch } from "@/lib/if-match";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { MailIcon, XIcon } from "lucide-react";

const reports = ["precondition_failed", "conflict"] as const;

/** Tells the admin why a refused change failed; the list is reloaded either way. */
function refused(title: string, error: unknown) {
  if (isPreconditionFailed(error)) {
    toast.add({
      type: "error",
      title,
      description:
        "Someone else changed this invitation. It's been reloaded: check it and try again.",
    });
  } else if (isErrorBody(error) && error.code === "conflict") {
    toast.add({ type: "error", title, description: errorMessage(error) });
  }
}

/** Resends or revokes a pending invitation once confirmed, sending the version listed. */
export function InvitationActions({ invitation }: { invitation: InvitationResponse }) {
  const queryClient = useQueryClient();
  const reload = () => {
    queryClient.invalidateQueries({ queryKey: getAccessInvitationsQueryKey() });
    queryClient.invalidateQueries({ queryKey: getAccessSubjectsQueryKey() });
  };
  const resend = useMutation({
    ...postAccessInvitationsByIdResendMutation(),
    meta: { reports },
    onSettled: reload,
  });
  const revoke = useMutation({
    ...deleteAccessInvitationsByIdMutation(),
    meta: { reports },
    onSettled: reload,
  });
  const request = { path: { id: invitation.id }, headers: ifMatch(invitation.version) };

  return (
    <span className="inline-flex gap-1">
      <AlertDialog>
        <AlertDialogTrigger
          render={
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={`Resend the invitation to ${invitation.email}`}
            />
          }
        >
          <MailIcon />
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Resend this invitation?</AlertDialogTitle>
            <AlertDialogDescription>
              {invitation.email} is mailed a fresh temporary password; the last one stops working.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={async () => {
                try {
                  await resend.mutateAsync(request);
                  toast.add({ type: "success", title: "Invitation resent" });
                } catch (error) {
                  refused("Couldn't resend", error);
                }
              }}
            >
              Resend
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <AlertDialog>
        <AlertDialogTrigger
          render={
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={`Revoke the invitation to ${invitation.email}`}
            />
          }
        >
          <XIcon />
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Revoke this invitation?</AlertDialogTitle>
            <AlertDialogDescription>
              {invitation.email}'s account is deleted at the identity provider and the roles the
              invitation gave are removed. Someone who has set their password can't be revoked:
              remove their roles instead.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={async () => {
                try {
                  await revoke.mutateAsync(request);
                  toast.add({ type: "success", title: "Invitation revoked" });
                } catch (error) {
                  refused("Couldn't revoke", error);
                }
              }}
            >
              Revoke
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </span>
  );
}
