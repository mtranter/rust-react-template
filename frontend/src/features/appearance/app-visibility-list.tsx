import { putAppearanceAppsMutation } from "@/api/@tanstack/react-query.gen";
import type { AppVisibility } from "@/api/types.gen";
import { ChangedMeanwhileAlert } from "@/components/changed-meanwhile-alert";
import { Switch } from "@/components/ui/switch";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { isForbidden, isPreconditionFailed } from "@/lib/api-errors";
import { ifMatch } from "@/lib/if-match";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { LucideIcon } from "lucide-react";
import { useState } from "react";
import { hiddenAppsQueryOptions } from "./use-hidden-apps";

export type ListedApp = { id: string; label: string; description: string; icon: LucideIcon };

const reports = ["precondition_failed"] as const;

/**
 * Every app, each with a switch that shows or hides it for the whole organisation at once. Hiding
 * only takes it out of the launcher, the sidebar and Go to: its operations still answer.
 */
export function AppVisibilityList({ apps }: { apps: ListedApp[] }) {
  const queryClient = useQueryClient();
  const queryKey = hiddenAppsQueryOptions().queryKey;
  const visibility = useQuery(hiddenAppsQueryOptions());
  const [changedMeanwhile, setChangedMeanwhile] = useState(false);
  const save = useMutation({ ...putAppearanceAppsMutation(), meta: { reports } });

  const toggle = async (current: AppVisibility, id: string, shown: boolean) => {
    setChangedMeanwhile(false);
    const hidden = shown
      ? current.hidden.filter((each) => each !== id)
      : [...current.hidden.filter((each) => each !== id), id];
    try {
      const saved = await save.mutateAsync({
        headers: ifMatch(String(current.version)),
        body: { hidden },
      });
      queryClient.setQueryData(queryKey, saved);
    } catch (error) {
      if (isPreconditionFailed(error)) {
        await queryClient.invalidateQueries({ queryKey });
        setChangedMeanwhile(true);
      }
    }
  };

  if (visibility.isError) {
    return (
      <p
        role="alert"
        className={isForbidden(visibility.error) ? "text-muted-foreground" : "text-destructive"}
      >
        Failed to load which apps are shown.
      </p>
    );
  }
  if (apps.length === 0) {
    return <p className="text-muted-foreground">There are no apps to show or hide.</p>;
  }

  return (
    <section className="space-y-3" aria-label="Apps">
      {changedMeanwhile && <ChangedMeanwhileAlert retry="save" subject="list of apps" />}
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>App</TableHead>
            <TableHead className="w-24 text-right">Shown</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {apps.map((app) => {
            const shown = !visibility.data?.hidden.includes(app.id);
            return (
              <TableRow key={app.id}>
                <TableCell>
                  <div className="flex items-center gap-3">
                    <app.icon className="size-4 text-muted-foreground" />
                    <div>
                      <div className="font-medium">{app.label}</div>
                      <div className="text-muted-foreground text-xs">{app.description}</div>
                    </div>
                  </div>
                </TableCell>
                <TableCell className="text-right">
                  <Switch
                    aria-label={`Show ${app.label}`}
                    checked={visibility.data ? shown : false}
                    disabled={!visibility.data || save.isPending}
                    onCheckedChange={(next) =>
                      visibility.data && toggle(visibility.data, app.id, next)
                    }
                  />
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </section>
  );
}
