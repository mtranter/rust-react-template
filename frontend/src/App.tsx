import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import {
  getNotesOptions,
  getNotesQueryKey,
  postNotesMutation,
} from "./api/@tanstack/react-query.gen";

// Minimal example wiring the generated client to TanStack Query (spec §8, §13):
// a paginated list (useQuery) and a create form (useMutation) that invalidates
// the list on success. Deliberately unstyled — this is a template seam, not a UI.
export function App() {
  const queryClient = useQueryClient();
  const notes = useQuery(getNotesOptions());
  const createNote = useMutation({
    ...postNotesMutation(),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: getNotesQueryKey() }),
  });

  const [authorId, setAuthorId] = useState("");
  const [title, setTitle] = useState("");

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    createNote.mutate(
      { body: { author_id: authorId, title } },
      {
        onSuccess: () => {
          setTitle("");
        },
      },
    );
  };

  return (
    <main>
      <h1>Notes</h1>

      <form onSubmit={onSubmit}>
        <input
          aria-label="author id"
          placeholder="author id"
          value={authorId}
          onChange={(e) => setAuthorId(e.target.value)}
        />
        <input
          aria-label="title"
          placeholder="title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
        />
        <button type="submit" disabled={createNote.isPending || !authorId || !title}>
          Add note
        </button>
      </form>

      {notes.isPending && <p>Loading…</p>}
      {notes.isError && <p role="alert">Failed to load notes.</p>}
      {notes.data && (
        <ul>
          {notes.data.items.map((note) => (
            <li key={note.id}>{note.title}</li>
          ))}
        </ul>
      )}
    </main>
  );
}
