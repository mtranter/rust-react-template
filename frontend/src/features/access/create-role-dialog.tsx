import { getAccessRolesQueryKey, postAccessRolesMutation } from "@/api/@tanstack/react-query.gen";
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
import { Textarea } from "@/components/ui/textarea";
import { toast } from "@/components/ui/toast";
import { errorMessage, isErrorBody, submitErrors } from "@/lib/api-errors";
import { useForm } from "@tanstack/react-form";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { PlusIcon } from "lucide-react";
import { useState } from "react";

const formFields = ["name", "description"] as const;

function errorsOf(errors: unknown[]) {
  return errors.map((message) => ({ message: String(message) }));
}

/** Creates a role grants can then name, in a dialog. */
export function CreateRoleDialog() {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button />}>
        <PlusIcon />
        Create role
      </DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Create role</DialogTitle>
          <DialogDescription>
            Its name is how grants name it, and can't be changed later.
          </DialogDescription>
        </DialogHeader>
        <CreateRoleForm onCreated={() => setOpen(false)} />
      </DialogContent>
    </Dialog>
  );
}

function CreateRoleForm({ onCreated }: { onCreated: () => void }) {
  const queryClient = useQueryClient();
  const create = useMutation({
    ...postAccessRolesMutation(),
    meta: { formFields, reports: ["conflict"] },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: getAccessRolesQueryKey() }),
  });

  const form = useForm({
    defaultValues: { name: "", description: "" },
    validators: {
      onSubmitAsync: async ({ value }) => {
        try {
          await create.mutateAsync({ body: { ...value, name: value.name.trim() } });
        } catch (error) {
          // A name taken is the name's fault, so it shows beside it.
          if (isErrorBody(error) && error.code === "conflict") {
            return { form: errorMessage(error), fields: { name: errorMessage(error) } };
          }
          return submitErrors(error);
        }
      },
    },
    onSubmit: () => {
      toast.add({ type: "success", title: "Role created" });
      onCreated();
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
      <form.Field name="name">
        {(field) => (
          <Field data-invalid={field.state.meta.errors.length > 0}>
            <FieldLabel htmlFor="role-name">Name</FieldLabel>
            <Input
              id="role-name"
              autoComplete="off"
              value={field.state.value}
              onChange={(e) => field.handleChange(e.target.value)}
              aria-invalid={field.state.meta.errors.length > 0}
              aria-describedby="role-name-error"
            />
            <FieldError id="role-name-error" errors={errorsOf(field.state.meta.errors)} />
          </Field>
        )}
      </form.Field>
      <form.Field name="description">
        {(field) => (
          <Field data-invalid={field.state.meta.errors.length > 0}>
            <FieldLabel htmlFor="role-description">Description</FieldLabel>
            <Textarea
              id="role-description"
              value={field.state.value}
              onChange={(e) => field.handleChange(e.target.value)}
              aria-invalid={field.state.meta.errors.length > 0}
              aria-describedby="role-description-error"
            />
            <FieldError id="role-description-error" errors={errorsOf(field.state.meta.errors)} />
          </Field>
        )}
      </form.Field>
      <DialogFooter>
        <DialogClose render={<Button variant="outline" />}>Cancel</DialogClose>
        <form.Subscribe selector={(state) => [state.isSubmitting, state.values.name] as const}>
          {([isSubmitting, name]) => (
            <Button type="submit" disabled={isSubmitting || !name.trim()}>
              Create
            </Button>
          )}
        </form.Subscribe>
      </DialogFooter>
    </form>
  );
}
