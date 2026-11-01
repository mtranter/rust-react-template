import { NewNoteForm, NotesTable } from "@/features/notes";
import { Link } from "@tanstack/react-router";

export function HomePage() {
  return (
    <main className="mx-auto max-w-4xl space-y-6 p-6">
      <div className="flex items-baseline justify-between">
        <h1 className="font-heading text-2xl font-semibold">Notes</h1>
        <Link to="/search" className="text-sm underline-offset-4 hover:underline">
          Search notes
        </Link>
      </div>
      <NewNoteForm />
      <NotesTable />
    </main>
  );
}
