import { getAccessSubjectsOptions } from "@/api/@tanstack/react-query.gen";
import type { SubjectResponse } from "@/api/types.gen";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { AssignRoleDialog } from "./assign-role-dialog";
import { LoadFailed } from "./load-failed";
import { UnassignRoleDialog } from "./unassign-role-dialog";

/** The most the server answers in a page: past it, a search narrows the list. */
const SHOWN = 100;

function nameOf(subject: SubjectResponse) {
  return subject.name ?? subject.email ?? subject.subject;
}

/** Who can be assigned roles, each with the roles they hold beside `user`, searched by `search`. */
export function SubjectsTable({
  search,
  onSearch,
}: {
  search?: string;
  onSearch: (search: string | undefined) => void;
}) {
  const subjects = useQuery({
    ...getAccessSubjectsOptions({ query: { search, limit: SHOWN } }),
    placeholderData: keepPreviousData,
  });

  return (
    <section className="space-y-3" aria-label="Users">
      <Input
        key={search ?? ""}
        aria-label="Search by name, email or user ID"
        placeholder="Search by name, email or user ID"
        className="max-w-sm"
        defaultValue={search ?? ""}
        onKeyDown={(event) => {
          if (event.key !== "Enter") return;
          onSearch(event.currentTarget.value.trim() || undefined);
        }}
        onBlur={(event) => {
          const value = event.currentTarget.value.trim() || undefined;
          if (value !== search) onSearch(value);
        }}
      />
      {subjects.isError ? (
        <LoadFailed error={subjects.error} what="the subjects" />
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>User</TableHead>
              <TableHead>Last seen</TableHead>
              <TableHead>Roles</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {subjects.isPending ? (
              <TableRow>
                <TableCell colSpan={3} className="text-muted-foreground">
                  Loading…
                </TableCell>
              </TableRow>
            ) : subjects.data.items.length === 0 ? (
              <TableRow>
                <TableCell colSpan={3} className="text-muted-foreground">
                  No one found.
                </TableCell>
              </TableRow>
            ) : (
              subjects.data.items.map((subject) => (
                <SubjectRow key={subject.subject} subject={subject} />
              ))
            )}
          </TableBody>
        </Table>
      )}
      {subjects.data && (subjects.data.total ?? 0) > subjects.data.items.length && (
        <p className="text-muted-foreground text-sm">
          Showing {subjects.data.items.length} of {subjects.data.total}: search to find the rest.
        </p>
      )}
    </section>
  );
}

function SubjectRow({ subject }: { subject: SubjectResponse }) {
  const label = nameOf(subject);
  return (
    <TableRow>
      <TableCell className="whitespace-normal">
        <div className="flex flex-col">
          <span className="flex items-center gap-2 font-medium" title={subject.subject}>
            {label}
            {subject.bootstrapAdmin && (
              <Badge
                variant="secondary"
                title="Named in the server's APP_BOOTSTRAP_ADMINS, so always an admin"
              >
                Always admin
              </Badge>
            )}
          </span>
          {subject.name && subject.email && (
            <span className="text-muted-foreground text-sm">{subject.email}</span>
          )}
        </div>
      </TableCell>
      <TableCell>
        {subject.lastSeen ? (
          <span title={new Date(subject.lastSeen).toLocaleString()}>
            {new Date(subject.lastSeen).toLocaleDateString()}
          </span>
        ) : (
          "Never signed in"
        )}
      </TableCell>
      <TableCell className="whitespace-normal">
        <ul className="flex flex-wrap items-center gap-1" aria-label={`Roles of ${label}`}>
          {subject.roles.map((held) => (
            <li key={held.role}>
              <Badge variant="outline" className="gap-0.5 pr-0.5">
                {held.role}
                {held.version && !(subject.bootstrapAdmin && held.role === "admin") && (
                  <UnassignRoleDialog
                    subject={subject.subject}
                    label={label}
                    role={held.role}
                    version={held.version}
                  />
                )}
              </Badge>
            </li>
          ))}
          <li>
            <AssignRoleDialog subject={subject.subject} label={label} />
          </li>
        </ul>
      </TableCell>
    </TableRow>
  );
}
