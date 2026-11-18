import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { DeleteRoleDialog } from "./delete-role-dialog";
import { useRoles } from "./use-roles";

/** Every role, by name. Not paged: an app has few. */
export function RolesTable() {
  const roles = useRoles();

  return (
    <section className="space-y-3" aria-label="Roles">
      {roles.isError ? (
        <p role="alert" className="text-destructive">
          Failed to load the roles.
        </p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Description</TableHead>
              <TableHead>Created by</TableHead>
              <TableHead>
                <span className="sr-only">Actions</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {roles.isPending ? (
              <TableRow>
                <TableCell colSpan={4} className="text-muted-foreground">
                  Loading…
                </TableCell>
              </TableRow>
            ) : (
              roles.data.items.map((role) => (
                <TableRow key={role.name}>
                  <TableCell className="font-medium">
                    <span className="flex items-center gap-2">
                      {role.name}
                      {role.builtIn && <Badge variant="secondary">Built in</Badge>}
                    </span>
                  </TableCell>
                  <TableCell className="whitespace-normal">{role.description}</TableCell>
                  <TableCell>{role.createdBy ?? (role.builtIn ? "" : "Default")}</TableCell>
                  <TableCell className="text-right">
                    {!role.builtIn && <DeleteRoleDialog role={role} />}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      )}
    </section>
  );
}
