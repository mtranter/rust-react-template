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

export function describe(grant: GrantResponse) {
  return `${grant.role} → ${grant.resource}:${grant.action} (${grant.scope})`;
}

/** Revokes a grant once confirmed, sending the version listed. */
export function RevokeGrantDialog({ grant }: { grant: GrantResponse }) {
  const queryClient = useQueryClient();
  const revoke = useMutation({
    ...deleteAccessGrantsByIdMutation(),
    meta: { reports: ["precondition_failed"] },
    onSettled: () => queryClient.invalidateQueries({ queryKey: getAccessGrantsQueryKey() }),
  });

  const confirm = async () => {
    try {
      await revoke.mutateAsync({ path: { id: grant.id }, headers: ifMatch(grant.version) });
      toast.add({ type: "success", title: "Grant revoked" });
    } catch (error) {
      // The list is reloaded either way, so it shows what the grant is now.
      if (isPreconditionFailed(error)) {
        toast.add({
          type: "error",
          title: "Couldn't revoke",
          description:
            "Someone else changed this grant. It's been reloaded: check it and revoke again.",
        });
      }
    }
  };

  return (
    <AlertDialog>
      <AlertDialogTrigger
        render={<Button variant="ghost" size="icon-sm" aria-label={`Revoke ${describe(grant)}`} />}
      >
        <Trash2Icon />
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Revoke this grant?</AlertDialogTitle>
          <AlertDialogDescription>
            {describe(grant)}. Every server stops allowing it within seconds.
            {grant.grantedBy ? "" : " A default grant, once revoked, isn't granted again."}
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
