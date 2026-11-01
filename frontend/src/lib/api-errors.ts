import type { ErrorBody } from "@/api";

// The generated client throws the parsed body for an API error, but a network
// failure still throws an `Error`, so narrow before trusting the shape.
export function isErrorBody(error: unknown): error is ErrorBody {
  return typeof error === "object" && error !== null && "code" in error && "message" in error;
}

export function errorMessage(error: unknown): string {
  if (isErrorBody(error)) return error.message;
  if (error instanceof Error) return error.message;
  return "Something went wrong.";
}

/**
 * For a TanStack Form `onSubmitAsync` that saves: puts each field error on the
 * input whose name is its `path`. Always an error, so a refused save never
 * reaches `onSubmit`; what no input shows is left to the global toast (see
 * `createQueryClient`).
 */
export function submitErrors(error: unknown): { form: string; fields: Record<string, string> } {
  const fields = isErrorBody(error) ? (error.fields ?? []) : [];
  return {
    form: errorMessage(error),
    fields: Object.fromEntries(fields.map((f) => [f.path, f.message])),
  };
}

/** True when `formFields` covers every field the error names, so the form shows it all. */
export function shownByForm(error: unknown, formFields: readonly string[] | undefined): boolean {
  if (!formFields || !isErrorBody(error) || !error.fields?.length) return false;
  return error.fields.every((f) => formFields.includes(f.path));
}
