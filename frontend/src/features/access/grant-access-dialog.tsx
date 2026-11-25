import {
  getAccessGrantsQueryKey,
  postAccessGrantsBulkMutation,
} from "@/api/@tanstack/react-query.gen";
import type { BulkGrantResponse } from "@/api/types.gen";
import { MultiSelect } from "@/components/multi-select";
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
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { toast } from "@/components/ui/toast";
import { submitErrors } from "@/lib/api-errors";
import { useForm } from "@tanstack/react-form";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { PlusIcon } from "lucide-react";
import { useState } from "react";
import { useCatalog } from "./catalog";
import { useRoles } from "./use-roles";

const formFields = ["role", "resources", "actions", "scopes"] as const;

function errorsOf(errors: unknown[]) {
  return errors.map((message) => ({ message: String(message) }));
}

/** Grants a role actions on resources, in scopes, all chosen from the catalog, in a dialog. */
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
    ...postAccessGrantsBulkMutation(),
    meta: { formFields },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: getAccessGrantsQueryKey() }),
  });

  /**
   * The actions the chosen resources have, grouped by resource, each option `resource/action`
   * so the same action can be chosen on one resource and not another; and the scopes they offer
   * between them, each named once.
   */
  const offered = (kinds: string[]) => {
    const chosen = resources.filter((r) => kinds.includes(r.resource));
    const scopes = new Map<string, { label: string; on: string[] }>([
      ["any", { label: "Any", on: [] }],
    ]);
    for (const resource of chosen) {
      for (const s of resource.scopes) {
        if (s.name === "any") continue;
        const seen = scopes.get(s.name) ?? { label: s.label, on: [] };
        seen.on.push(resource.label);
        scopes.set(s.name, seen);
      }
    }
    return {
      actionGroups: chosen.map((r) => ({
        label: r.label,
        options: r.actions.map((a) => ({
          value: `${r.resource}/${a.name}`,
          label: a.reachable ? a.label : `${a.label} (no one can yet)`,
        })),
      })),
      // A scope is named per resource (`owner` on notes and on conversations), so say on which.
      scopes: [...scopes].map(([value, s]) => ({
        value,
        label: s.on.length > 0 ? `${s.label} (${s.on.join(", ")})` : s.label,
      })),
    };
  };

  /**
   * Each `resource/action` chosen, sent as one bulk grant per set of actions: resources given
   * the same actions go together, and the server makes only what each resource has.
   */
  const grantChosen = async (value: {
    role: string;
    actions: string[];
    scopes: string[];
  }) => {
    const byResource = new Map<string, string[]>();
    for (const chosen of value.actions) {
      const [resource, action] = chosen.split("/");
      byResource.set(resource, [...(byResource.get(resource) ?? []), action]);
    }
    const byActions = new Map<string, { resources: string[]; actions: string[] }>();
    for (const [resource, actions] of byResource) {
      const key = [...actions].sort().join(",");
      const group = byActions.get(key) ?? { resources: [], actions };
      group.resources.push(resource);
      byActions.set(key, group);
    }
    // Each group asks only for the scopes one of its resources offers, which the server requires.
    const offers = (kind: string, scope: string) =>
      scope === "any" ||
      (resources.find((r) => r.resource === kind)?.scopes.some((s) => s.name === scope) ?? false);
    const granted: BulkGrantResponse["granted"] = [];
    const skipped: BulkGrantResponse["skipped"] = [];
    for (const group of byActions.values()) {
      const scopes = value.scopes.filter((s) => group.resources.some((r) => offers(r, s)));
      if (scopes.length === 0) continue;
      const result = await grant.mutateAsync({
        body: { role: value.role, scopes, ...group },
      });
      granted.push(...result.granted);
      skipped.push(...result.skipped);
    }
    return { granted, skipped };
  };

  const form = useForm({
    defaultValues: {
      role: "",
      resources: [] as string[],
      actions: [] as string[],
      scopes: ["any"] as string[],
    },
    validators: {
      onSubmitAsync: async ({ value }) => {
        try {
          const result = await grantChosen(value);
          const skipped = result.skipped.length;
          toast.add({
            type: "success",
            title: `Granted ${result.granted.length}`,
            ...(skipped > 0 && { description: `${skipped} already granted` }),
          });
        } catch (error) {
          return submitErrors(error);
        }
      },
    },
    onSubmit: () => onGranted(),
  });

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
        name="resources"
        listeners={{
          onChange: ({ value, fieldApi }) => {
            const still = offered(value);
            const keep = (chosen: string[], options: { value: string }[]) =>
              chosen.filter((c) => options.some((o) => o.value === c));
            fieldApi.form.setFieldValue("actions", (a) =>
              keep(
                a,
                still.actionGroups.flatMap((g) => g.options),
              ),
            );
            fieldApi.form.setFieldValue("scopes", (s) => keep(s, still.scopes));
          },
        }}
      >
        {(field) => (
          <Field data-invalid={field.state.meta.errors.length > 0}>
            <FieldLabel htmlFor="grant-resources">On</FieldLabel>
            <MultiSelect
              id="grant-resources"
              placeholder="Choose what"
              groups={capabilities.map((c) => ({
                label: c.label,
                options: c.resources.map((r) => ({ value: r.resource, label: r.label })),
              }))}
              value={field.state.value}
              onChange={field.handleChange}
              aria-invalid={field.state.meta.errors.length > 0}
              aria-describedby="grant-resources-error"
            />
            <FieldError id="grant-resources-error" errors={errorsOf(field.state.meta.errors)} />
          </Field>
        )}
      </form.Field>
      <form.Subscribe selector={(state) => state.values.resources}>
        {(kinds) => {
          const { actionGroups, scopes } = offered(kinds);
          return (
            <>
              <form.Field name="actions">
                {(field) => (
                  <Field data-invalid={field.state.meta.errors.length > 0}>
                    <FieldLabel htmlFor="grant-actions">Can</FieldLabel>
                    <MultiSelect
                      id="grant-actions"
                      placeholder="Choose actions"
                      disabled={kinds.length === 0}
                      groups={actionGroups}
                      value={field.state.value}
                      onChange={field.handleChange}
                      aria-invalid={field.state.meta.errors.length > 0}
                      aria-describedby="grant-actions-error"
                    />
                    <FieldError
                      id="grant-actions-error"
                      errors={errorsOf(field.state.meta.errors)}
                    />
                  </Field>
                )}
              </form.Field>
              <form.Field name="scopes">
                {(field) => (
                  <Field data-invalid={field.state.meta.errors.length > 0}>
                    <FieldLabel htmlFor="grant-scopes">Which</FieldLabel>
                    <MultiSelect
                      id="grant-scopes"
                      placeholder="Choose which"
                      disabled={kinds.length === 0}
                      groups={[{ options: scopes }]}
                      value={field.state.value}
                      onChange={field.handleChange}
                      aria-invalid={field.state.meta.errors.length > 0}
                      aria-describedby="grant-scopes-error"
                    />
                    <FieldError
                      id="grant-scopes-error"
                      errors={errorsOf(field.state.meta.errors)}
                    />
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
            [
              state.isSubmitting,
              state.values.role,
              state.values.actions.length,
              state.values.scopes.length,
            ] as const
          }
        >
          {([isSubmitting, role, actions, scopes]) => (
            <Button type="submit" disabled={isSubmitting || !role || !actions || !scopes}>
              Grant
            </Button>
          )}
        </form.Subscribe>
      </DialogFooter>
    </form>
  );
}
