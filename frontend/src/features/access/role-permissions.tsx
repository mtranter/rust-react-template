import {
  deleteAccessGrantsByIdMutation,
  getAccessCatalogQueryKey,
  getAccessGrantsQueryKey,
  postAccessGrantsMutation,
} from "@/api/@tanstack/react-query.gen";
import { getAccessGrants } from "@/api/sdk.gen";
import type { CatalogResourceResponse, GrantResponse } from "@/api/types.gen";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { toast } from "@/components/ui/toast";
import { isPreconditionFailed } from "@/lib/api-errors";
import { ifMatch } from "@/lib/if-match";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { type Labels, useCatalog } from "./catalog";
import { LoadFailed } from "./load-failed";
import { RevokeGrantDialog } from "./revoke-grant-dialog";
import { useRoles } from "./use-roles";

/** The most the server answers in a page. */
const PAGE = 100;

/** Every grant of `role`, read page by page: a role's grants are few, but may pass one page. */
function useRoleGrants(role: string) {
  return useQuery({
    queryKey: getAccessGrantsQueryKey({ query: { role, limit: PAGE } }),
    queryFn: async ({ signal }) => {
      const grants: GrantResponse[] = [];
      for (let offset = 0; ; offset += PAGE) {
        const { data } = await getAccessGrants({
          query: { role, limit: PAGE, offset },
          signal,
          throwOnError: true,
        });
        grants.push(...data.items);
        if (data.items.length < PAGE || grants.length >= (data.total ?? 0)) return grants;
      }
    },
  });
}

/** What `role` may do, Capability by Capability, changed in place: each change grants or revokes. */
export function RolePermissions({ role }: { role: string }) {
  const roles = useRoles();
  const { query: catalog, capabilities, labels } = useCatalog();
  const grants = useRoleGrants(role);
  const found = roles.data?.items.find((r) => r.name === role);

  if (roles.isError || catalog.isError || grants.isError) {
    return (
      <LoadFailed
        error={roles.error ?? catalog.error ?? grants.error}
        what="this role's permissions"
      />
    );
  }
  if (roles.isPending || catalog.isPending || grants.isPending) {
    return <p className="text-muted-foreground">Loading…</p>;
  }
  if (!found) {
    return (
      <p role="alert" className="text-muted-foreground">
        There's no role called {role}.
      </p>
    );
  }

  const stale = grants.data.filter((g) => g.stale);
  return (
    <div className="space-y-6">
      <div className="space-y-1">
        {found.description && <p>{found.description}</p>}
        <p className="text-muted-foreground text-sm">
          {role === "admin"
            ? "Admins can always manage access, whatever is ticked here. Changes take effect within a few seconds."
            : role === "user"
              ? "Everyone who signs in has this role. Changes take effect within a few seconds."
              : "Changes take effect within a few seconds for everyone with this role."}
        </p>
      </div>
      {capabilities.map((c) => (
        <section key={c.capability} aria-label={c.label} className="space-y-2">
          <h2 className="font-heading text-lg font-semibold">{c.label}</h2>
          <div className="divide-y rounded-lg border">
            {c.resources.map((resource) => (
              <ResourcePermissions
                key={resource.resource}
                role={role}
                resource={resource}
                grants={grants.data.filter((g) => g.resource === resource.resource && !g.stale)}
              />
            ))}
          </div>
        </section>
      ))}
      {stale.length > 0 && (
        <section aria-label="No longer used" className="space-y-2">
          <h2 className="font-heading text-lg font-semibold">No longer used</h2>
          <p className="text-muted-foreground text-sm">
            Their modules no longer have these, so they allow nothing. They're safe to revoke.
          </p>
          <ul className="divide-y rounded-lg border">
            {stale.map((grant) => (
              <StaleGrant key={grant.id} grant={grant} labels={labels} />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

function StaleGrant({ grant, labels }: { grant: GrantResponse; labels: Labels }) {
  return (
    <li className="flex items-center justify-between gap-3 px-4 py-2">
      <span className="flex items-center gap-2">
        {labels.resource(grant.resource)}: {labels.action(grant.resource, grant.action)}
        <Badge variant="destructive">No longer used</Badge>
      </span>
      <RevokeGrantDialog grant={grant} labels={labels} />
    </li>
  );
}

/** A grant's scope, or none for an action not granted. */
type Choice = string | null;

function ResourcePermissions({
  role,
  resource,
  grants,
}: {
  role: string;
  resource: CatalogResourceResponse;
  grants: GrantResponse[];
}) {
  const change = useChangeGrant(role, resource.resource);
  const scoped = resource.scopes.length > 1;

  return (
    <div className="grid gap-3 px-4 py-3 sm:grid-cols-[12rem_1fr]">
      <h3 className="font-medium">{resource.label}</h3>
      <div
        className={
          scoped ? "grid gap-x-6 gap-y-2 sm:grid-cols-2" : "flex flex-wrap gap-x-6 gap-y-2"
        }
      >
        {resource.actions.map((action) => {
          const held = grants.filter((g) => g.action === action.name);
          const current: Choice = held.some((g) => g.scope === "any")
            ? "any"
            : (held[0]?.scope ?? null);
          const name = `${resource.label}: ${action.label}`;
          const busy = change.pending === action.name;
          return scoped ? (
            <div key={action.name} className="flex items-center justify-between gap-3 text-sm">
              <span aria-hidden="true">{action.label}</span>
              <NativeSelect
                aria-label={name}
                className="w-48"
                disabled={busy}
                value={current ?? ""}
                onChange={(e) => change.to(action.name, held, e.target.value || null)}
              >
                <NativeSelectOption value="">Not allowed</NativeSelectOption>
                {resource.scopes.map((scope) => (
                  <NativeSelectOption key={scope.name} value={scope.name}>
                    {scope.label}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </div>
          ) : (
            <span key={action.name} className="flex items-center gap-2 text-sm">
              <Checkbox
                aria-label={name}
                disabled={busy}
                checked={current !== null}
                onCheckedChange={(checked) => change.to(action.name, held, checked ? "any" : null)}
              />
              <span aria-hidden="true">{action.label}</span>
            </span>
          );
        })}
      </div>
    </div>
  );
}

/**
 * Grants `role` an action of `resource` at a scope, then revokes whatever else it held of that
 * action, so the role never briefly loses what it keeps; `null` revokes them all.
 */
function useChangeGrant(role: string, resource: string) {
  const queryClient = useQueryClient();
  const [pending, setPending] = useState<string>();
  const grant = useMutation({ ...postAccessGrantsMutation() });
  const revoke = useMutation({
    ...deleteAccessGrantsByIdMutation(),
    meta: { reports: ["precondition_failed"] },
  });

  const to = async (action: string, held: GrantResponse[], scope: Choice) => {
    setPending(action);
    try {
      if (scope && !held.some((g) => g.scope === scope)) {
        await grant.mutateAsync({ body: { role, resource, action, scope } });
      }
      for (const g of held.filter((g) => g.scope !== scope)) {
        await revoke.mutateAsync({ path: { id: g.id }, headers: ifMatch(g.version) });
      }
      toast.add({ type: "success", title: "Saved" });
    } catch (error) {
      if (isPreconditionFailed(error)) {
        toast.add({
          type: "error",
          title: "Couldn't save",
          description: "Someone else changed this meanwhile. It's been reloaded: try again.",
        });
      }
    } finally {
      // Held until the reload, so the control never shows what it was before the change.
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: getAccessGrantsQueryKey() }),
        queryClient.invalidateQueries({ queryKey: getAccessCatalogQueryKey() }),
      ]);
      setPending(undefined);
    }
  };

  return { pending, to };
}
