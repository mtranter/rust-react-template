import type { ErrorBody, Theme, ThemeInput } from "@/api";
import { http, HttpResponse } from "msw";

/**
 * `ui-settings` as the server answers: the theme versioned with `If-Match` (its number, quoted),
 * a value containing `;` refused by its path. `saves` holds every body it accepted.
 */
export function uiSettingsApi(stored: Partial<Theme> = {}) {
  const theme: { current: Theme } = {
    current: { version: 0, light: {}, dark: {}, shared: {}, ...stored },
  };
  const saves: ThemeInput[] = [];
  const handlers = [
    http.get("*/api/ui-settings/theme", () => HttpResponse.json(theme.current)),
    http.put("*/api/ui-settings/theme", async ({ request }) => {
      if (request.headers.get("If-Match") !== `"${theme.current.version}"`) {
        return HttpResponse.json<ErrorBody>(
          { code: "precondition_failed", message: "stale", requestId: "test" },
          { status: 412 },
        );
      }
      const body = (await request.json()) as ThemeInput;
      const fields = Object.entries(body).flatMap(([map, values]) =>
        Object.entries(values as Record<string, string>)
          .filter(([, v]) => v.includes(";"))
          .map(([name]) => ({ path: `${map}.${name}`, message: "must not contain `;`" })),
      );
      if (fields.length) {
        return HttpResponse.json<ErrorBody>(
          { code: "validation_error", message: "invalid theme", requestId: "test", fields },
          { status: 400 },
        );
      }
      saves.push(body);
      theme.current = {
        ...body,
        version: theme.current.version + 1,
        updatedAt: "2026-11-27T09:00:00Z",
        updatedBy: "ada",
      };
      return HttpResponse.json(theme.current);
    }),
  ];
  return { handlers, theme, saves };
}
