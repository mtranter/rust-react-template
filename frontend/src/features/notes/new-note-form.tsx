import {
  getNotesQueryKey,
  getUsersOptions,
  postNotesMutation,
} from "@/api/@tanstack/react-query.gen";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { submitErrors } from "@/lib/api-errors";
import { useForm } from "@tanstack/react-form";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

const formFields = ["authorId", "title"] as const;

function errorsOf(errors: unknown[]) {
  return errors.map((message) => ({ message: String(message) }));
}

export function NewNoteForm() {
  const queryClient = useQueryClient();
  // One page at the server's maximum: enough for a picker until users gets a search endpoint.
  const users = useQuery(getUsersOptions({ query: { limit: 100, sort: ["displayName"] } }));
  const authors = (users.data?.items ?? []).map((user) => ({
    value: user.id,
    label: user.displayName,
  }));
  const createNote = useMutation({
    ...postNotesMutation(),
    meta: { formFields },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: getNotesQueryKey() }),
  });

  const form = useForm({
    defaultValues: { authorId: "", title: "" },
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
          className="grid gap-4 sm:grid-cols-[1fr_1fr_auto] sm:items-start"
        >
          <form.Field name="authorId">
            {(field) => (
              <Field data-invalid={field.state.meta.errors.length > 0}>
                <FieldLabel htmlFor="author">Author</FieldLabel>
                <Select
                  items={authors}
                  value={field.state.value || null}
                  onValueChange={(value) => field.handleChange(value ?? "")}
                >
                  <SelectTrigger id="author" className="w-full">
                    <SelectValue placeholder={users.isPending ? "Loading…" : "Choose an author"} />
                  </SelectTrigger>
                  <SelectContent>
                    {authors.map((author) => (
                      <SelectItem key={author.value} value={author.value}>
                        {author.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <FieldError errors={errorsOf(field.state.meta.errors)} />
              </Field>
            )}
          </form.Field>
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
          <form.Subscribe
            selector={(state) => [state.isSubmitting, state.values.authorId, state.values.title]}
          >
            {([isSubmitting, authorId, title]) => (
              <Button
                type="submit"
                className="sm:mt-7"
                disabled={!!isSubmitting || !authorId || !title}
              >
                Add note
              </Button>
            )}
          </form.Subscribe>
        </form>
      </CardContent>
    </Card>
  );
}
