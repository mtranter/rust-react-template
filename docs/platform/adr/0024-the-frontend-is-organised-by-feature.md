# The frontend is organised by feature, mirroring backend Modules

Each backend Module with a UI gets `src/features/<module>/`: its components, hooks and tests, imported only through its `index.ts`. Features never import each other; shared code goes in `src/components/`, `src/hooks/` or `src/lib/`. Pages (`src/pages/`) are thin: they lay out features and own URL concerns. The frontend's boundaries so match the backend's, and a feature can be read, changed or removed without the others.

Routing is TanStack Router, code-based in `src/router.tsx`, with no file-route codegen; `router.test.tsx` checks each URL lands on its page. The UI is shadcn/ui (Base UI primitives, Tailwind v4, `base-nova`), vendored whole as source in `src/components/ui/`: existing components are composed rather than restyled, and theming is the CSS variables in `src/index.css`. Server-paged lists use the shared `data-table/` over TanStack Table. Forms use TanStack Form, saving in `onSubmitAsync` and mapping server field errors onto inputs (ADR 0022); any other failed mutation is toasted by the shared `QueryClient`.

## Considered options

- **Organising by technical kind** (`components/`, `hooks/`, `api/` across the app). Every feature change touches every folder, and nothing keeps features apart.
- **File-based routes.** Another codegen step for a handful of routes.

## Consequences

- `pnpm run lint:imports` enforces the feature boundaries (ADR 0017).
- Vendored shadcn code trips some Biome rules; `biome.json` disables them for the vendored paths only. Refreshing from upstream overwrites local edits, so they are re-checked.
