import {
  deleteAccessAssignmentsMutation,
  getAccessSubjectsQueryKey,
} from "@/api/@tanstack/react-query.gen";
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
import { XIcon } from "lucide-react";

/** Removes a role from a subject once confirmed, sending the assignment's version listed. */
export function UnassignRoleDialog({
  subject,
  label,
  role,
  version,
}: {
  subject: string;
  label: string;
  role: string;
  version: string;
}) {
  const queryClient = useQueryClient();
  const unassign = useMutation({
    ...deleteAccessAssignmentsMutation(),
    meta: { reports: ["precondition_failed", "conflict", "validation_error"] },
    onSettled: () => queryClient.invalidateQueries({ queryKey: getAccessSubjectsQueryKey() }),
  });

  const confirm = async () => {
    try {
      await unassign.mutateAsync({ query: { subject, role }, headers: ifMatch(version) });
      toast.add({ type: "success", title: "Role removed" });
    } catch (error) {
      // The list is reloaded either way, so it shows what the user holds now.
      if (isPreconditionFailed(error)) {
        toast.add({
          type: "error",
          title: "Couldn't remove",
          description:
            "Someone else changed this user's roles. They've been reloaded: check them and remove again.",
        });
      } else if (isErrorBody(error)) {
        toast.add({ type: "error", title: "Couldn't remove", description: errorMessage(error) });
      }
    }
  };

  return (
    <AlertDialog>
      <AlertDialogTrigger
        render={
          <Button variant="ghost" size="icon-xs" aria-label={`Remove ${role} from ${label}`} />
        }
      >
        <XIcon />
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remove this role?</AlertDialogTitle>
          <AlertDialogDescription>
            {label} stops holding {role} on every server within seconds.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction variant="destructive" onClick={confirm}>
            Remove
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
