import {
  getAccessInvitationsQueryKey,
  getAccessSubjectsQueryKey,
  postAccessInvitationsMutation,
} from "@/api/@tanstack/react-query.gen";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
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
import { Field, FieldError, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { toast } from "@/components/ui/toast";
import { errorMessage, isErrorBody, submitErrors } from "@/lib/api-errors";
import { useForm } from "@tanstack/react-form";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { MailPlusIcon } from "lucide-react";
import { useState } from "react";
import { useRoles } from "./use-roles";

const formFields = ["email", "name", "roles"] as const;

function errorsOf(errors: unknown[]) {
  return errors.map((message) => ({ message: String(message) }));
}

/** Invites someone by email: the identity provider creates their account and mails them. */
export function InviteUserDialog() {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button />}>
        <MailPlusIcon />
        Invite user
      </DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Invite user</DialogTitle>
          <DialogDescription>
            They're emailed a temporary password to sign in with, and hold the roles you choose from
            now on.
          </DialogDescription>
        </DialogHeader>
        <InviteUserForm onInvited={() => setOpen(false)} />
      </DialogContent>
    </Dialog>
  );
}

function InviteUserForm({ onInvited }: { onInvited: () => void }) {
  const queryClient = useQueryClient();
  const roles = useRoles();
  const invite = useMutation({
    ...postAccessInvitationsMutation(),
    meta: { formFields, reports: ["conflict", "not_configured", "unavailable"] },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: getAccessInvitationsQueryKey() });
      queryClient.invalidateQueries({ queryKey: getAccessSubjectsQueryKey() });
    },
  });

  const form = useForm({
    defaultValues: { email: "", name: "", roles: [] as string[] },
    validators: {
      onSubmitAsync: async ({ value }) => {
        try {
          await invite.mutateAsync({
            body: { ...value, email: value.email.trim(), name: value.name.trim() },
          });
        } catch (error) {
          // Each of these is about who is invited, so it shows beside the email.
          if (
            isErrorBody(error) &&
            ["conflict", "not_configured", "unavailable"].includes(error.code)
          ) {
            return { form: errorMessage(error), fields: { email: errorMessage(error) } };
          }
          return submitErrors(error);
        }
      },
    },
    onSubmit: () => {
      toast.add({ type: "success", title: "Invitation sent" });
      onInvited();
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
      <form.Field name="email">
        {(field) => (
          <Field data-invalid={field.state.meta.errors.length > 0}>
            <FieldLabel htmlFor="invite-email">Email</FieldLabel>
            <Input
              id="invite-email"
              type="email"
              autoComplete="off"
              value={field.state.value}
              onChange={(e) => field.handleChange(e.target.value)}
              aria-invalid={field.state.meta.errors.length > 0}
              aria-describedby="invite-email-error"
            />
            <FieldError id="invite-email-error" errors={errorsOf(field.state.meta.errors)} />
          </Field>
        )}
      </form.Field>
      <form.Field name="name">
        {(field) => (
          <Field data-invalid={field.state.meta.errors.length > 0}>
            <FieldLabel htmlFor="invite-name">Name</FieldLabel>
            <Input
              id="invite-name"
              autoComplete="off"
              value={field.state.value}
              onChange={(e) => field.handleChange(e.target.value)}
              aria-invalid={field.state.meta.errors.length > 0}
              aria-describedby="invite-name-error"
            />
            <FieldError id="invite-name-error" errors={errorsOf(field.state.meta.errors)} />
          </Field>
        )}
      </form.Field>
      <form.Field name="roles">
        {(field) => (
          <FieldSet data-invalid={field.state.meta.errors.length > 0}>
            <FieldLegend variant="label">Roles</FieldLegend>
            {roles.data?.items
              // Every signed-in user holds `user`: it is never assigned.
              .filter((r) => r.name !== "user")
              .map((r) => (
                // Wrapping it labels the checkbox, which is a button, not a labelable input.
                <FieldLabel key={r.name}>
                  <Checkbox
                    checked={field.state.value.includes(r.name)}
                    onCheckedChange={(checked) =>
                      field.handleChange(
                        checked
                          ? [...field.state.value, r.name]
                          : field.state.value.filter((name) => name !== r.name),
                      )
                    }
                  />
                  {r.name}
                </FieldLabel>
              ))}
            <FieldError errors={errorsOf(field.state.meta.errors)} />
          </FieldSet>
        )}
      </form.Field>
      <DialogFooter>
        <DialogClose render={<Button variant="outline" />}>Cancel</DialogClose>
        <form.Subscribe selector={(state) => [state.isSubmitting, state.values.email] as const}>
          {([isSubmitting, email]) => (
            <Button type="submit" disabled={isSubmitting || !email.trim()}>
              Invite
            </Button>
          )}
        </form.Subscribe>
      </DialogFooter>
    </form>
  );
}
