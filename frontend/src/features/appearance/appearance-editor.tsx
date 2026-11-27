import { getAccessMePermissionsOptions } from "@/api/@tanstack/react-query.gen";
import { putAppearanceThemeMutation } from "@/api/@tanstack/react-query.gen";
import { ChangedMeanwhileAlert } from "@/components/changed-meanwhile-alert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { Slider } from "@/components/ui/slider";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { toast } from "@/components/ui/toast";
import { errorMessage, isPreconditionFailed, submitErrors } from "@/lib/api-errors";
import {
  type Theme,
  type ThemeTokens,
  applyTheme,
  endPreview,
  previewTheme,
  themeQueryOptions,
} from "@/lib/appearance";
import { ifMatch } from "@/lib/if-match";
import { useForm, useStore } from "@tanstack/react-form";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { ThemePreview } from "./theme-preview";
import {
  type Scheme,
  type ShadowPreset,
  colourGroups,
  draftOf,
  fontChoices,
  fontLabels,
  fontTokens,
  formFields,
  overridesOf,
  readDefaults,
  shadowPresetOf,
  shadowPresets,
  shadowTokens,
  toHex,
  tokenLabel,
} from "./tokens";

const reports = ["precondition_failed"] as const;

function errorsOf(errors: unknown[]) {
  return errors.map((message) => ({ message: String(message) }));
}

/** The organisation's theme: colours per scheme, shape, shadows and fonts, previewed live. */
export function AppearanceEditor() {
  const theme = useQuery(themeQueryOptions());
  const permissions = useQuery({
    ...getAccessMePermissionsOptions(),
    retry: false,
    staleTime: 60_000,
  });
  const permitted = permissions.data?.permissions.some(
    (p) => p.resource === "theme" && p.action === "update",
  );

  if (permissions.isError || permitted === false) {
    return (
      <p role="alert" className="text-muted-foreground">
        You may not change the appearance: ask an admin for access.
      </p>
    );
  }
  if (theme.isPending || permissions.isPending)
    return <p className="text-muted-foreground">Loading…</p>;
  return <Editor theme={theme.data ?? { version: 0, light: {}, dark: {}, shared: {} }} />;
}

function Editor({ theme }: { theme: Theme }) {
  const queryClient = useQueryClient();
  const queryKey = themeQueryOptions().queryKey;
  const defaults = useMemo(readDefaults, []);
  const [version, setVersion] = useState(theme.version);
  const [changedMeanwhile, setChangedMeanwhile] = useState(false);

  const save = useMutation({
    ...putAppearanceThemeMutation(),
    meta: { formFields, reports },
  });

  const saved = (answer: Theme) => {
    setVersion(answer.version);
    queryClient.setQueryData(queryKey, answer);
    applyTheme(answer);
    void queryClient.invalidateQueries({ queryKey });
  };

  const reload = async () => {
    try {
      const latest = await queryClient.fetchQuery({ ...themeQueryOptions(), staleTime: 0 });
      if (latest) setVersion(latest.version);
      setChangedMeanwhile(true);
    } catch (error) {
      toast.add({ type: "error", title: "Couldn't reload", description: errorMessage(error) });
    }
  };

  const send = async (body: ThemeTokens) => {
    setChangedMeanwhile(false);
    try {
      saved(await save.mutateAsync({ headers: ifMatch(String(version)), body }));
    } catch (error) {
      if (isPreconditionFailed(error)) await reload();
      throw error;
    }
  };

  const form = useForm({
    defaultValues: draftOf(theme),
    validators: {
      onSubmitAsync: async ({ value }) => {
        try {
          await send(overridesOf(value));
        } catch (error) {
          return submitErrors(error);
        }
      },
    },
    onSubmit: ({ value }) => {
      toast.add({ type: "success", title: "Appearance saved" });
      form.reset(value);
    },
  });

  const values = useStore(form.store, (s) => s.values);
  const dirty = useStore(form.store, (s) => !s.isDefaultValue);
  useEffect(() => {
    if (dirty) previewTheme(overridesOf(values));
    else endPreview();
  }, [values, dirty]);
  useEffect(() => endPreview, []);

  const resetToDefaults = async () => {
    try {
      await send({ light: {}, dark: {}, shared: {} });
      form.reset(draftOf(null));
      toast.add({ type: "success", title: "Appearance reset to the defaults" });
    } catch {}
  };

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        form.handleSubmit();
      }}
      className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_22rem] lg:items-start"
    >
      <div className="grid gap-6">
        {changedMeanwhile && <ChangedMeanwhileAlert retry="save" subject="theme" />}
        <Card>
          <CardHeader>
            <CardTitle>Colours</CardTitle>
            <CardDescription>
              Any CSS colour. Leave one empty to keep the default, shown greyed.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Tabs defaultValue="light">
              <TabsList>
                <TabsTrigger value="light">Light</TabsTrigger>
                <TabsTrigger value="dark">Dark</TabsTrigger>
              </TabsList>
              {(["light", "dark"] as const).map((scheme) => (
                <TabsContent key={scheme} value={scheme} className="grid gap-6 pt-2">
                  {colourGroups.map((group) => (
                    <fieldset key={group.label} className="grid gap-3">
                      <legend className="font-medium text-sm">{group.label}</legend>
                      <p className="text-muted-foreground text-xs">{group.description}</p>
                      <div className="grid gap-3 sm:grid-cols-2">
                        {group.tokens.map((token) => (
                          <form.Field key={token} name={`${scheme}.${token}`}>
                            {(field) => (
                              <ColourInput
                                scheme={scheme}
                                token={token}
                                value={field.state.value}
                                fallback={defaults[scheme][token] ?? ""}
                                errors={errorsOf(field.state.meta.errors)}
                                onChange={field.handleChange}
                              />
                            )}
                          </form.Field>
                        ))}
                      </div>
                    </fieldset>
                  ))}
                </TabsContent>
              ))}
            </Tabs>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Shape</CardTitle>
            <CardDescription>How round corners are, from square to soft.</CardDescription>
          </CardHeader>
          <CardContent>
            <form.Field name="shared.radius">
              {(field) => {
                const rem = Number.parseFloat(field.state.value || defaults.shared.radius || "0.5");
                return (
                  <Field data-invalid={field.state.meta.errors.length > 0}>
                    <FieldLabel id="radius-label">Corner radius: {rem}rem</FieldLabel>
                    <Slider
                      aria-labelledby="radius-label"
                      min={0}
                      max={1.5}
                      step={0.125}
                      value={[rem]}
                      onValueChange={(v) => field.handleChange(`${Array.isArray(v) ? v[0] : v}rem`)}
                    />
                    <FieldError errors={errorsOf(field.state.meta.errors)} />
                  </Field>
                );
              }}
            </form.Field>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Shadows</CardTitle>
            <CardDescription>How far cards, menus and dialogs lift off the page.</CardDescription>
          </CardHeader>
          <CardContent>
            <Field>
              <FieldLabel htmlFor="shadow-preset">Shadows</FieldLabel>
              <NativeSelect
                id="shadow-preset"
                value={shadowPresetOf(values.shared)}
                onChange={(e) => {
                  const preset = shadowPresets[e.target.value as ShadowPreset];
                  if (!preset) return;
                  for (const t of shadowTokens)
                    form.setFieldValue(`shared.${t}`, preset.values[t] ?? "");
                }}
              >
                {Object.entries(shadowPresets).map(([name, preset]) => (
                  <NativeSelectOption key={name} value={name}>
                    {preset.label}
                  </NativeSelectOption>
                ))}
                {shadowPresetOf(values.shared) === "custom" && (
                  <NativeSelectOption value="custom" disabled>
                    Custom
                  </NativeSelectOption>
                )}
              </NativeSelect>
            </Field>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Fonts</CardTitle>
            <CardDescription>
              System fonts, or the bundled Geist: nothing is downloaded.
            </CardDescription>
          </CardHeader>
          <CardContent className="grid gap-4 sm:grid-cols-3">
            {fontTokens.map((token) => (
              <form.Field key={token} name={`shared.${token}`}>
                {(field) => {
                  const choices = fontChoices[token];
                  const known = choices.some((c) => c.value === field.state.value);
                  return (
                    <Field data-invalid={field.state.meta.errors.length > 0}>
                      <FieldLabel htmlFor={`font-${token}`}>{fontLabels[token]}</FieldLabel>
                      <NativeSelect
                        id={`font-${token}`}
                        className="w-full"
                        value={field.state.value}
                        onChange={(e) => field.handleChange(e.target.value)}
                      >
                        {choices.map((c) => (
                          <NativeSelectOption key={c.label} value={c.value}>
                            {c.label}
                          </NativeSelectOption>
                        ))}
                        {!known && (
                          <NativeSelectOption value={field.state.value}>Custom</NativeSelectOption>
                        )}
                      </NativeSelect>
                      <FieldError errors={errorsOf(field.state.meta.errors)} />
                    </Field>
                  );
                }}
              </form.Field>
            ))}
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-4 lg:sticky lg:top-4">
        <ThemePreview />
        <div className="flex flex-wrap justify-end gap-2">
          <AlertDialog>
            <AlertDialogTrigger render={<Button type="button" variant="ghost" />}>
              Reset to defaults
            </AlertDialogTrigger>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Reset to the defaults?</AlertDialogTitle>
                <AlertDialogDescription>
                  Every colour, the corners, shadows and fonts go back to the app's own, for
                  everyone.
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>Cancel</AlertDialogCancel>
                <AlertDialogAction onClick={resetToDefaults}>Reset</AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
          <Button type="button" variant="outline" disabled={!dirty} onClick={() => form.reset()}>
            Discard changes
          </Button>
          <form.Subscribe selector={(s) => s.isSubmitting}>
            {(isSubmitting) => (
              <Button type="submit" disabled={isSubmitting || !dirty}>
                Save
              </Button>
            )}
          </form.Subscribe>
        </div>
      </div>
    </form>
  );
}

function ColourInput({
  scheme,
  token,
  value,
  fallback,
  errors,
  onChange,
}: {
  scheme: Scheme;
  token: string;
  value: string;
  fallback: string;
  errors: { message: string }[];
  onChange: (value: string) => void;
}) {
  const id = `${scheme}-${token}`;
  const label = tokenLabel(token);
  return (
    <Field data-invalid={errors.length > 0}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <div className="flex items-center gap-2">
        <input
          type="color"
          aria-label={`Pick ${label}`}
          className="h-8 w-10 shrink-0 cursor-pointer rounded-md border border-input bg-transparent p-0.5"
          value={toHex(value || fallback)}
          onChange={(e) => onChange(e.target.value)}
        />
        <Input
          id={id}
          autoComplete="off"
          spellCheck={false}
          className="font-mono text-xs"
          placeholder={fallback}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          aria-invalid={errors.length > 0}
          aria-describedby={`${id}-error`}
        />
      </div>
      <FieldError id={`${id}-error`} errors={errors} />
    </Field>
  );
}
