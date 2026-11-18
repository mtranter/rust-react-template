import { getNotesQueryKey, postNotesMutation } from "@/api/@tanstack/react-query.gen";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { submitErrors } from "@/lib/api-errors";
import { useForm } from "@tanstack/react-form";
import { useMutation, useQueryClient } from "@tanstack/react-query";

const formFields = ["title"] as const;

function errorsOf(errors: unknown[]) {
  return errors.map((message) => ({ message: String(message) }));
}

export function NewNoteForm() {
  const queryClient = useQueryClient();
  const createNote = useMutation({
    ...postNotesMutation(),
    meta: { formFields },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: getNotesQueryKey() }),
  });

  const form = useForm({
    // The server makes the signed-in caller the author.
    defaultValues: { title: "" },
    validators: {
      // Saving is the validation: the server's field errors land on the inputs.
      onSubmitAsync: async ({ value }) => {
        try {
          await createNote.mutateAsync({ body: value });
        } catch (error) {
          return submitErrors(error);
        }
      },
    },
    onSubmit: ({ formApi }) => formApi.setFieldValue("title", ""),
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>New note</CardTitle>
      </CardHeader>
      <CardContent>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            form.handleSubmit();
          }}
          className="grid gap-4 sm:grid-cols-[1fr_auto] sm:items-start"
        >
          <form.Field name="title">
            {(field) => (
              <Field data-invalid={field.state.meta.errors.length > 0}>
                <FieldLabel htmlFor="title">Title</FieldLabel>
                <Input
                  id="title"
                  value={field.state.value}
                  onChange={(e) => field.handleChange(e.target.value)}
                  aria-invalid={field.state.meta.errors.length > 0}
                  aria-describedby="title-error"
                />
                <FieldError id="title-error" errors={errorsOf(field.state.meta.errors)} />
              </Field>
            )}
          </form.Field>
          <form.Subscribe selector={(state) => [state.isSubmitting, state.values.title] as const}>
            {([isSubmitting, title]) => (
              <Button type="submit" className="sm:mt-7" disabled={isSubmitting || !title}>
                Add note
              </Button>
            )}
          </form.Subscribe>
        </form>
      </CardContent>
    </Card>
  );
}
