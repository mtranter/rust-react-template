import { getAccessGrantsQueryKey, postAccessGrantsMutation } from "@/api/@tanstack/react-query.gen";
import type { CatalogResourceResponse } from "@/api/types.gen";
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
import {
  NativeSelect,
  NativeSelectOptGroup,
  NativeSelectOption,
} from "@/components/ui/native-select";
import { toast } from "@/components/ui/toast";
import { submitErrors } from "@/lib/api-errors";
import { useForm } from "@tanstack/react-form";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { PlusIcon } from "lucide-react";
import { useState } from "react";
import { useCatalog } from "./catalog";
import { useRoles } from "./use-roles";

const formFields = ["role", "resource", "action", "scope"] as const;

function errorsOf(errors: unknown[]) {
  return errors.map((message) => ({ message: String(message) }));
}

/** Grants a role an action, chosen from the catalog, in a dialog. */
export function GrantAccessDialog() {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button />}>
        <PlusIcon />
        Grant access
      </DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Grant access</DialogTitle>
          <DialogDescription>
            Lets everyone with the role do it, within a few seconds. To grant a role not listed,
            create it on the Roles tab first.
          </DialogDescription>
        </DialogHeader>
        <GrantAccessForm onGranted={() => setOpen(false)} />
      </DialogContent>
    </Dialog>
  );
}

function GrantAccessForm({ onGranted }: { onGranted: () => void }) {
  const queryClient = useQueryClient();
  const { capabilities, resources } = useCatalog();
  const roles = useRoles();
  const grant = useMutation({
    ...postAccessGrantsMutation(),
    meta: { formFields },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: getAccessGrantsQueryKey() }),
  });

  const form = useForm({
    defaultValues: { role: "", resource: "", action: "", scope: "any" },
    validators: {
      onSubmitAsync: async ({ value }) => {
        try {
          await grant.mutateAsync({ body: value });
        } catch (error) {
          return submitErrors(error);
        }
      },
    },
    onSubmit: () => {
      toast.add({ type: "success", title: "Access granted" });
      onGranted();
    },
  });

  const resourceOf = (kind: string): CatalogResourceResponse | undefined =>
    resources.find((r) => r.resource === kind);

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        form.handleSubmit();
      }}
      className="grid gap-4"
    >
      <form.Field name="role">
        {(field) => (
          <Field data-invalid={field.state.meta.errors.length > 0}>
            <FieldLabel htmlFor="grant-role">Role</FieldLabel>
            <NativeSelect
              id="grant-role"
              className="w-full"
              value={field.state.value}
              onChange={(e) => field.handleChange(e.target.value)}
              aria-invalid={field.state.meta.errors.length > 0}
              aria-describedby="grant-role-error"
            >
              <NativeSelectOption value="">Choose a role</NativeSelectOption>
              {roles.data?.items.map((r) => (
                <NativeSelectOption key={r.name} value={r.name}>
                  {r.name}
                </NativeSelectOption>
              ))}
            </NativeSelect>
            <FieldError id="grant-role-error" errors={errorsOf(field.state.meta.errors)} />
          </Field>
        )}
      </form.Field>
      <form.Field
        name="resource"
        listeners={{
          onChange: ({ fieldApi }) => {
            fieldApi.form.setFieldValue("action", "");
            fieldApi.form.setFieldValue("scope", "any");
          },
        }}
      >
        {(field) => (
          <Field data-invalid={field.state.meta.errors.length > 0}>
            <FieldLabel htmlFor="grant-resource">On</FieldLabel>
            <NativeSelect
              id="grant-resource"
              className="w-full"
              value={field.state.value}
              onChange={(e) => field.handleChange(e.target.value)}
              aria-invalid={field.state.meta.errors.length > 0}
              aria-describedby="grant-resource-error"
            >
              <NativeSelectOption value="">Choose what</NativeSelectOption>
              {capabilities.map((c) => (
                <NativeSelectOptGroup key={c.capability} label={c.label}>
                  {c.resources.map((r) => (
                    <NativeSelectOption key={r.resource} value={r.resource}>
                      {r.label}
                    </NativeSelectOption>
                  ))}
                </NativeSelectOptGroup>
              ))}
            </NativeSelect>
            <FieldError id="grant-resource-error" errors={errorsOf(field.state.meta.errors)} />
          </Field>
        )}
      </form.Field>
      <form.Subscribe selector={(state) => state.values.resource}>
        {(kind) => {
          const resource = resourceOf(kind);
          return (
            <>
              <form.Field name="action">
                {(field) => (
                  <Field data-invalid={field.state.meta.errors.length > 0}>
                    <FieldLabel htmlFor="grant-action">Can</FieldLabel>
                    <NativeSelect
                      id="grant-action"
                      className="w-full"
                      disabled={!resource}
                      value={field.state.value}
                      onChange={(e) => field.handleChange(e.target.value)}
                      aria-invalid={field.state.meta.errors.length > 0}
                      aria-describedby="grant-action-error"
                    >
                      <NativeSelectOption value="">Choose an action</NativeSelectOption>
                      {resource?.actions.map((a) => (
                        <NativeSelectOption key={a.name} value={a.name}>
                          {a.label}
                          {a.reachable ? "" : " (no one can yet)"}
                        </NativeSelectOption>
                      ))}
                    </NativeSelect>
                    <FieldError
                      id="grant-action-error"
                      errors={errorsOf(field.state.meta.errors)}
                    />
                  </Field>
                )}
              </form.Field>
              <form.Field name="scope">
                {(field) => (
                  <Field data-invalid={field.state.meta.errors.length > 0}>
                    <FieldLabel htmlFor="grant-scope">Which</FieldLabel>
                    <NativeSelect
                      id="grant-scope"
                      className="w-full"
                      disabled={!resource}
                      value={field.state.value}
                      onChange={(e) => field.handleChange(e.target.value)}
                      aria-invalid={field.state.meta.errors.length > 0}
                      aria-describedby="grant-scope-error"
                    >
                      {(resource?.scopes ?? [{ name: "any", label: "Any" }]).map((scope) => (
                        <NativeSelectOption key={scope.name} value={scope.name}>
                          {scope.label}
                        </NativeSelectOption>
                      ))}
                    </NativeSelect>
                    <FieldError id="grant-scope-error" errors={errorsOf(field.state.meta.errors)} />
                  </Field>
                )}
              </form.Field>
            </>
          );
        }}
      </form.Subscribe>
      <DialogFooter>
        <DialogClose render={<Button variant="outline" />}>Cancel</DialogClose>
        <form.Subscribe
          selector={(state) =>
            [state.isSubmitting, state.values.role, state.values.action] as const
          }
        >
          {([isSubmitting, role, action]) => (
            <Button type="submit" disabled={isSubmitting || !role || !action}>
              Grant
            </Button>
          )}
        </form.Subscribe>
      </DialogFooter>
    </form>
  );
}
