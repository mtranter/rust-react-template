import {
  getAccessSubjectsQueryKey,
  postAccessAssignmentsMutation,
} from "@/api/@tanstack/react-query.gen";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { toast } from "@/components/ui/toast";
import { errorMessage, isErrorBody, submitErrors } from "@/lib/api-errors";
import { useForm } from "@tanstack/react-form";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { PlusIcon, UserPlusIcon } from "lucide-react";
import { useState } from "react";
import { useRoles } from "./use-roles";

const formFields = ["subject", "role"] as const;

function errorsOf(errors: unknown[]) {
  return errors.map((message) => ({ message: String(message) }));
}

/**
 * Assigns a role in a dialog: to `subject`, a user listed, or, without one, to a `sub` typed in,
 * who may not have signed in yet.
 */
export function AssignRoleDialog({ subject, label }: { subject?: string; label?: string }) {
  const [open, setOpen] = useState(false);
  const title = subject ? `Assign a role to ${label ?? subject}` : "Add user";
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      {subject ? (
        <DialogTrigger render={<Button variant="ghost" size="icon-sm" aria-label={title} />}>
          <PlusIcon />
        </DialogTrigger>
      ) : (
        <DialogTrigger render={<Button />}>
          <UserPlusIcon />
          Add user
        </DialogTrigger>
      )}
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>
            {subject
              ? "It takes effect within a few seconds."
              : "Give someone a role before or after they first sign in. Use the user ID your sign-in provider shows for them."}
          </DialogDescription>
        </DialogHeader>
        <AssignRoleForm subject={subject} onAssigned={() => setOpen(false)} />
      </DialogContent>
    </Dialog>
  );
}

function AssignRoleForm({ subject, onAssigned }: { subject?: string; onAssigned: () => void }) {
  const queryClient = useQueryClient();
  const roles = useRoles();
  const assign = useMutation({
    ...postAccessAssignmentsMutation(),
    meta: { formFields, reports: ["conflict"] },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: getAccessSubjectsQueryKey() }),
  });

  const form = useForm({
    defaultValues: { subject: subject ?? "", role: "" },
    validators: {
      onSubmitAsync: async ({ value }) => {
        try {
          await assign.mutateAsync({ body: { ...value, subject: value.subject.trim() } });
        } catch (error) {
          // Holding the role already is the role's fault, so it shows beside it.
          if (isErrorBody(error) && error.code === "conflict") {
            return { form: errorMessage(error), fields: { role: errorMessage(error) } };
          }
          return submitErrors(error);
        }
      },
    },
    onSubmit: () => {
      toast.add({ type: "success", title: "Role assigned" });
      onAssigned();
    },
  });

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        form.handleSubmit();
      }}
      className="grid gap-4"
    >
      <form.Field name="subject">
        {(field) =>
          subject ? null : (
            <Field data-invalid={field.state.meta.errors.length > 0}>
              <FieldLabel htmlFor="assign-subject">User ID</FieldLabel>
              <Input
                id="assign-subject"
                autoComplete="off"
                placeholder="auth0|0123456789"
                value={field.state.value}
                onChange={(e) => field.handleChange(e.target.value)}
                aria-invalid={field.state.meta.errors.length > 0}
                aria-describedby="assign-subject-error"
              />
              <FieldError id="assign-subject-error" errors={errorsOf(field.state.meta.errors)} />
            </Field>
          )
        }
      </form.Field>
      <form.Field name="role">
        {(field) => (
          <Field data-invalid={field.state.meta.errors.length > 0}>
            <FieldLabel htmlFor="assign-role">Role</FieldLabel>
            <NativeSelect
              id="assign-role"
              className="w-full"
              value={field.state.value}
              onChange={(e) => field.handleChange(e.target.value)}
              aria-invalid={field.state.meta.errors.length > 0}
              aria-describedby="assign-role-error"
            >
              <NativeSelectOption value="">Choose a role</NativeSelectOption>
              {roles.data?.items
                // Every signed-in user holds `user`: it is never assigned.
                .filter((r) => r.name !== "user")
                .map((r) => (
                  <NativeSelectOption key={r.name} value={r.name}>
                    {r.name}
                  </NativeSelectOption>
                ))}
            </NativeSelect>
            <FieldError id="assign-role-error" errors={errorsOf(field.state.meta.errors)} />
          </Field>
        )}
      </form.Field>
      <DialogFooter>
        <DialogClose render={<Button variant="outline" />}>Cancel</DialogClose>
        <form.Subscribe
          selector={(state) =>
            [state.isSubmitting, state.values.subject, state.values.role] as const
          }
        >
          {([isSubmitting, typed, role]) => (
            <Button type="submit" disabled={isSubmitting || !typed.trim() || !role}>
              Assign
            </Button>
          )}
        </form.Subscribe>
      </DialogFooter>
    </form>
  );
}
