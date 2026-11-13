import {
  getNotesByIdOptions,
  getNotesQueryKey,
  patchNotesByIdMutation,
} from "@/api/@tanstack/react-query.gen";
import type { NoteResponse } from "@/api/types.gen";
import { ChangedMeanwhileAlert } from "@/components/changed-meanwhile-alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { toast } from "@/components/ui/toast";
import { errorMessage, isPreconditionFailed, submitErrors } from "@/lib/api-errors";
import { ifMatch } from "@/lib/if-match";
import { useForm } from "@tanstack/react-form";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { PencilIcon } from "lucide-react";
import { useState } from "react";

const formFields = ["title", "body"] as const;

function errorsOf(errors: unknown[]) {
  return errors.map((message) => ({ message: String(message) }));
}

/** Edits a note's title and body in a dialog, opened from its row. */
export function EditNoteDialog({ note }: { note: NoteResponse }) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger
        render={<Button variant="ghost" size="icon-sm" aria-label={`Edit ${note.title}`} />}
      >
        <PencilIcon />
      </DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Edit note</DialogTitle>
        </DialogHeader>
        <EditNoteForm note={note} onSaved={() => setOpen(false)} />
      </DialogContent>
    </Dialog>
  );
}

function EditNoteForm({ note, onSaved }: { note: NoteResponse; onSaved: () => void }) {
  const queryClient = useQueryClient();
  const edit = useMutation({
    ...patchNotesByIdMutation(),
    meta: { formFields, reports: ["precondition_failed"] },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: getNotesQueryKey() }),
  });

  // The version the user's edit is based on: the one opened, until a conflict reloads it.
  const [version, setVersion] = useState(note.version);
  const [changedMeanwhile, setChangedMeanwhile] = useState(false);

  const reload = async () => {
    void queryClient.invalidateQueries({ queryKey: getNotesQueryKey() });
    try {
      const latest = await queryClient.fetchQuery(getNotesByIdOptions({ path: { id: note.id } }));
      setVersion(latest.version);
      setChangedMeanwhile(true);
    } catch (error) {
      toast.add({ type: "error", title: "Couldn't reload", description: errorMessage(error) });
    }
  };

  const form = useForm({
    defaultValues: { title: note.title, body: note.body },
    validators: {
      onSubmitAsync: async ({ value }) => {
        setChangedMeanwhile(false);
        try {
          await edit.mutateAsync({
            path: { id: note.id },
            headers: ifMatch(version),
            body: value,
          });
        } catch (error) {
          if (isPreconditionFailed(error)) await reload();
          return submitErrors(error);
        }
      },
    },
    onSubmit: () => {
      toast.add({ type: "success", title: "Note saved" });
      onSaved();
    },
  });

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        form.handleSubmit();
      }}
      className="grid gap-4"
    >
      {changedMeanwhile && <ChangedMeanwhileAlert retry="save" subject="note" />}
      <form.Field name="title">
        {(field) => (
          <Field data-invalid={field.state.meta.errors.length > 0}>
            <FieldLabel htmlFor="edit-note-title">Title</FieldLabel>
            <Input
              id="edit-note-title"
              autoComplete="off"
              value={field.state.value}
              onChange={(e) => field.handleChange(e.target.value)}
              aria-invalid={field.state.meta.errors.length > 0}
              aria-describedby="edit-note-title-error"
            />
            <FieldError id="edit-note-title-error" errors={errorsOf(field.state.meta.errors)} />
          </Field>
        )}
      </form.Field>
      <form.Field name="body">
        {(field) => (
          <Field data-invalid={field.state.meta.errors.length > 0}>
            <FieldLabel htmlFor="edit-note-body">Body</FieldLabel>
            <Textarea
              id="edit-note-body"
              rows={6}
              value={field.state.value}
              onChange={(e) => field.handleChange(e.target.value)}
              aria-invalid={field.state.meta.errors.length > 0}
              aria-describedby="edit-note-body-error"
            />
            <FieldError id="edit-note-body-error" errors={errorsOf(field.state.meta.errors)} />
          </Field>
        )}
      </form.Field>
      <DialogFooter>
        <DialogClose render={<Button variant="outline" />}>Cancel</DialogClose>
        <form.Subscribe selector={(state) => [state.isSubmitting, state.values.title]}>
          {([isSubmitting, title]) => (
            <Button type="submit" disabled={!!isSubmitting || !title}>
              Save
            </Button>
          )}
        </form.Subscribe>
      </DialogFooter>
    </form>
  );
}
