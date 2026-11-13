/** The `If-Match` header for a change to a resource at `version` (a strong entity-tag, as its `ETag`). */
export function ifMatch(version: string) {
  return { "If-Match": `"${version}"` };
}
