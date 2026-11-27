import {
  getConnectionsByKeyOptions,
  getConnectionsByKeyQueryKey,
  getConnectionsQueryKey,
  patchConnectionsByKeyMutation,
  postConnectionsByKeyTestMutation,
  putConnectionsByKeySecretsByNameMutation,
} from "@/api/@tanstack/react-query.gen";
import type { ConnectionResponse, SecretResponse } from "@/api/types.gen";
import { ChangedMeanwhileAlert } from "@/components/changed-meanwhile-alert";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { toast } from "@/components/ui/toast";
import { errorMessage, isForbidden, isPreconditionFailed, submitErrors } from "@/lib/api-errors";
import { ifMatch } from "@/lib/if-match";
import { useForm } from "@tanstack/react-form";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CircleCheckIcon, CircleXIcon, PlugIcon } from "lucide-react";
import { useState } from "react";
import { ConnectionStatusBadge } from "./connection-status-badge";

function errorsOf(errors: unknown[]) {
  return errors.map((message) => ({ message: String(message) }));
}

/**
 * One Connection: its URL and fields, its secrets (write-only: an input never shows a stored
 * value, only whether one is set), and **Test connection**.
 */
export function ConnectionForm({ connectionKey }: { connectionKey: string }) {
  const options = getConnectionsByKeyOptions({ path: { key: connectionKey } });
  const connection = useQuery(options);

  if (connection.isError) {
    return (
      <p
        role="alert"
        className={isForbidden(connection.error) ? "text-muted-foreground" : "text-destructive"}
      >
        {isForbidden(connection.error)
          ? "You may not manage integrations: ask an admin for access."
          : `Failed to load the integration: ${errorMessage(connection.error)}`}
      </p>
    );
  }
  if (connection.isPending) return <p className="text-muted-foreground">Loading…</p>;
  return <Loaded connection={connection.data} />;
}

/** Every change answers the Connection as it now is, so the next one goes at its new version. */
function useKeepAnswer(key: string) {
  const queryClient = useQueryClient();
  return (answer: ConnectionResponse) => {
    queryClient.setQueryData(getConnectionsByKeyQueryKey({ path: { key } }), answer);
    void queryClient.invalidateQueries({ queryKey: getConnectionsQueryKey() });
  };
}

function Loaded({ connection }: { connection: ConnectionResponse }) {
  return (
    <div className="grid gap-6">
      <TestConnection connection={connection} />
      <Settings key={connection.version} connection={connection} />
      {connection.secrets.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Credentials</CardTitle>
            <CardDescription>
              Stored encrypted, and never shown again: enter a new value to replace one.
            </CardDescription>
          </CardHeader>
          <CardContent className="grid gap-4">
            {connection.secrets.map((secret) => (
              <SecretInput key={secret.name} connection={connection} secret={secret} />
            ))}
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function Settings({ connection }: { connection: ConnectionResponse }) {
  const queryClient = useQueryClient();
  const keep = useKeepAnswer(connection.key);
  const [changedMeanwhile, setChangedMeanwhile] = useState(false);
  const formFields = ["url", ...connection.fields.map((f) => `fields.${f.name}`)];
  const update = useMutation({
    ...patchConnectionsByKeyMutation(),
    meta: { formFields, reports: ["precondition_failed"] },
    onSuccess: keep,
  });

  const form = useForm({
    defaultValues: {
      url: connection.url ?? "",
      fields: Object.fromEntries(connection.fields.map((f) => [f.name, f.value ?? ""])),
    },
    validators: {
      onSubmitAsync: async ({ value }) => {
        setChangedMeanwhile(false);
        try {
          await update.mutateAsync({
            path: { key: connection.key },
            headers: ifMatch(connection.version),
            // An empty URL clears it.
            body: { url: value.url.trim(), fields: value.fields },
          });
        } catch (error) {
          if (isPreconditionFailed(error)) {
            await queryClient.invalidateQueries({
              queryKey: getConnectionsByKeyQueryKey({ path: { key: connection.key } }),
            });
            setChangedMeanwhile(true);
          }
          return submitErrors(error);
        }
      },
    },
    onSubmit: () => toast.add({ type: "success", title: "Integration saved" }),
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>Settings</CardTitle>
        <CardDescription>
          Where {connection.label} is, as its administrator gives it.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            form.handleSubmit();
          }}
          className="grid gap-4"
        >
          {changedMeanwhile && <ChangedMeanwhileAlert retry="save" subject="integration" />}
          <form.Field name="url">
            {(field) => (
              <Field data-invalid={field.state.meta.errors.length > 0}>
                <FieldLabel htmlFor="connection-url">URL</FieldLabel>
                <Input
                  id="connection-url"
                  type="url"
                  autoComplete="off"
                  placeholder="https://api.example.com"
                  value={field.state.value}
                  onChange={(e) => field.handleChange(e.target.value)}
                  aria-invalid={field.state.meta.errors.length > 0}
                  aria-describedby="connection-url-error"
                />
                <FieldError id="connection-url-error" errors={errorsOf(field.state.meta.errors)} />
              </Field>
            )}
          </form.Field>
          {connection.fields.map((declared) => (
            <form.Field key={declared.name} name={`fields.${declared.name}`}>
              {(field) => (
                <Field data-invalid={field.state.meta.errors.length > 0}>
                  <FieldLabel htmlFor={`connection-field-${declared.name}`}>
                    {declared.label}
                  </FieldLabel>
                  <Input
                    id={`connection-field-${declared.name}`}
                    autoComplete="off"
                    value={field.state.value ?? ""}
                    onChange={(e) => field.handleChange(e.target.value)}
                    aria-invalid={field.state.meta.errors.length > 0}
                  />
                  <FieldError errors={errorsOf(field.state.meta.errors)} />
                </Field>
              )}
            </form.Field>
          ))}
          <form.Subscribe selector={(state) => state.isSubmitting}>
            {(isSubmitting) => (
              <div>
                <Button type="submit" disabled={isSubmitting}>
                  Save
                </Button>
              </div>
            )}
          </form.Subscribe>
        </form>
      </CardContent>
    </Card>
  );
}

function setOn(secret: SecretResponse) {
  if (!secret.set) return "Not set.";
  const when = secret.setAt ? new Date(secret.setAt).toLocaleString() : undefined;
  return ["Set", secret.setBy && `by ${secret.setBy}`, when && `on ${when}`]
    .filter(Boolean)
    .join(" ")
    .concat(".");
}

function SecretInput({
  connection,
  secret,
}: {
  connection: ConnectionResponse;
  secret: SecretResponse;
}) {
  const keep = useKeepAnswer(connection.key);
  const id = `connection-secret-${secret.name}`;
  const set = useMutation({
    ...putConnectionsByKeySecretsByNameMutation(),
    meta: { formFields: ["value"], reports: ["not_configured", "precondition_failed"] },
    onSuccess: keep,
  });
  const verb = secret.set ? "Replace" : "Set";

  const form = useForm({
    defaultValues: { value: "" },
    validators: {
      onSubmitAsync: async ({ value }) => {
        try {
          await set.mutateAsync({
            path: { key: connection.key, name: secret.name },
            headers: ifMatch(connection.version),
            body: { value: value.value },
          });
        } catch (error) {
          const { form, fields } = submitErrors(error);
          return { form, fields: { value: fields.value ?? form } };
        }
      },
    },
    onSubmit: () => {
      form.reset();
      toast.add({ type: "success", title: `${secret.label} saved` });
    },
  });

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        form.handleSubmit();
      }}
    >
      <form.Field name="value">
        {(field) => (
          <Field data-invalid={field.state.meta.errors.length > 0}>
            <FieldLabel htmlFor={id}>{secret.label}</FieldLabel>
            <div className="flex gap-2">
              <Input
                id={id}
                type="password"
                autoComplete="new-password"
                placeholder={secret.set ? "••••••••" : undefined}
                value={field.state.value}
                onChange={(e) => field.handleChange(e.target.value)}
                aria-invalid={field.state.meta.errors.length > 0}
                aria-describedby={`${id}-set ${id}-error`}
              />
              <form.Subscribe
                selector={(state) => [state.isSubmitting, state.values.value] as const}
              >
                {([isSubmitting, value]) => (
                  <Button
                    type="submit"
                    variant="outline"
                    disabled={isSubmitting || !value}
                    aria-label={`${verb} ${secret.label}`}
                  >
                    {verb}
                  </Button>
                )}
              </form.Subscribe>
            </div>
            <FieldDescription id={`${id}-set`}>{setOn(secret)}</FieldDescription>
            <FieldError id={`${id}-error`} errors={errorsOf(field.state.meta.errors)} />
          </Field>
        )}
      </form.Field>
    </form>
  );
}

function TestConnection({ connection }: { connection: ConnectionResponse }) {
  const keep = useKeepAnswer(connection.key);
  const [refused, setRefused] = useState<string>();
  const test = useMutation({
    ...postConnectionsByKeyTestMutation(),
    meta: { reports: ["not_configured", "validation_error"] },
    onMutate: () => setRefused(undefined),
    onSuccess: keep,
    onError: (error) => setRefused(errorMessage(error)),
  });
  const testedAt = connection.testedAt ? new Date(connection.testedAt).toLocaleString() : undefined;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          Status <ConnectionStatusBadge status={connection.status} />
        </CardTitle>
        <CardDescription>
          {connection.healthCheck
            ? `Testing sends ${connection.healthCheck.method} ${connection.healthCheck.path} to ${connection.label}.`
            : `${connection.label} declares no test.`}
        </CardDescription>
      </CardHeader>
      <CardContent className="grid gap-4">
        <output aria-label="Test result" className="block">
          {refused ? (
            <Alert variant="destructive">
              <CircleXIcon />
              <AlertTitle>Couldn't test it</AlertTitle>
              <AlertDescription>{refused}</AlertDescription>
            </Alert>
          ) : connection.status === "ok" ? (
            <Alert variant="success">
              <CircleCheckIcon />
              <AlertTitle>OK</AlertTitle>
              <AlertDescription>
                {connection.label} answered as expected{testedAt && ` at ${testedAt}`}.
              </AlertDescription>
            </Alert>
          ) : connection.status === "failing" ? (
            <Alert variant="destructive">
              <CircleXIcon />
              <AlertTitle>Failing</AlertTitle>
              <AlertDescription>{connection.lastError}</AlertDescription>
            </Alert>
          ) : connection.status === "not_configured" ? (
            <p className="text-muted-foreground text-sm">
              Give it a URL and its credentials, then test it.
            </p>
          ) : (
            <p className="text-muted-foreground text-sm">Not tested since it last changed.</p>
          )}
        </output>
        {connection.healthCheck && (
          <div>
            <Button
              variant="outline"
              disabled={test.isPending || connection.status === "not_configured"}
              onClick={() => test.mutate({ path: { key: connection.key } })}
            >
              <PlugIcon />
              Test connection
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
