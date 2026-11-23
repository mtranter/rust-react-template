import {
  deleteAccessGrantsByIdMutation,
  getAccessGrantsQueryKey,
} from "@/api/@tanstack/react-query.gen";
import type { GrantResponse } from "@/api/types.gen";
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
import { isPreconditionFailed } from "@/lib/api-errors";
import { ifMatch } from "@/lib/if-match";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Trash2Icon } from "lucide-react";
import type { Labels } from "./catalog";

/** A grant in words: `support can Edit Notes (Notes they wrote)`. */
export function describe(grant: GrantResponse, labels: Labels) {
  const action = labels.action(grant.resource, grant.action);
  const resource = labels.resource(grant.resource);
  return `${grant.role} can ${action} ${resource} (${labels.scope(grant.resource, grant.scope)})`;
}

/** Revokes a grant once confirmed, sending the version listed. */
export function RevokeGrantDialog({ grant, labels }: { grant: GrantResponse; labels: Labels }) {
  const queryClient = useQueryClient();
  const revoke = useMutation({
    ...deleteAccessGrantsByIdMutation(),
    meta: { reports: ["precondition_failed"] },
    onSettled: () => queryClient.invalidateQueries({ queryKey: getAccessGrantsQueryKey() }),
  });

  const confirm = async () => {
    try {
      await revoke.mutateAsync({ path: { id: grant.id }, headers: ifMatch(grant.version) });
      toast.add({ type: "success", title: "Permission revoked" });
    } catch (error) {
      // The list is reloaded either way, so it shows what the grant is now.
      if (isPreconditionFailed(error)) {
        toast.add({
          type: "error",
          title: "Couldn't revoke",
          description:
            "Someone else changed this permission. It's been reloaded: check it and revoke again.",
        });
      }
    }
  };

  return (
    <AlertDialog>
      <AlertDialogTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Revoke: ${describe(grant, labels)}`}
          />
        }
      >
        <Trash2Icon />
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Revoke this permission?</AlertDialogTitle>
          <AlertDialogDescription>
            {describe(grant, labels)}. It stops working within a few seconds.
            {grant.grantedBy ? "" : " It's built in, so once revoked it isn't granted again."}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction variant="destructive" onClick={confirm}>
            Revoke
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
