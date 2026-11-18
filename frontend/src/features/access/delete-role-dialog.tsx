import {
  deleteAccessRolesByNameMutation,
  getAccessRolesQueryKey,
} from "@/api/@tanstack/react-query.gen";
import type { RoleResponse } from "@/api/types.gen";
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
import { Trash2Icon } from "lucide-react";

/** Deletes a role once confirmed, sending the version listed. */
export function DeleteRoleDialog({ role }: { role: RoleResponse }) {
  const queryClient = useQueryClient();
  const remove = useMutation({
    ...deleteAccessRolesByNameMutation(),
    meta: { reports: ["precondition_failed", "conflict"] },
    onSettled: () => queryClient.invalidateQueries({ queryKey: getAccessRolesQueryKey() }),
  });

  const confirm = async () => {
    try {
      await remove.mutateAsync({ path: { name: role.name }, headers: ifMatch(role.version) });
      toast.add({ type: "success", title: "Role deleted" });
    } catch (error) {
      // The list is reloaded either way, so it shows what the role is now.
      if (isPreconditionFailed(error)) {
        toast.add({
          type: "error",
          title: "Couldn't delete",
          description:
            "Someone else changed this role. It's been reloaded: check it and delete again.",
        });
      } else if (isErrorBody(error) && error.code === "conflict") {
        toast.add({ type: "error", title: "Couldn't delete", description: errorMessage(error) });
      }
    }
  };

  return (
    <AlertDialog>
      <AlertDialogTrigger
        render={<Button variant="ghost" size="icon-sm" aria-label={`Delete ${role.name}`} />}
      >
        <Trash2Icon />
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete this role?</AlertDialogTitle>
          <AlertDialogDescription>
            {role.name}. A role a grant still names can't be deleted: revoke its grants first.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction variant="destructive" onClick={confirm}>
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
