import { registry } from "@/shell/capabilities";
import { http, HttpResponse } from "msw";
import { beforeEach } from "vitest";
import { server } from "../../vitest.setup";

/** Every action an Area of the registry needs: what a caller who may use every Area holds. */
export const everyAreasNeeds = [...registry.apps, ...registry.platform].flatMap((capability) =>
  capability.areas.map((area) => area.needs),
);

/** The caller holds `permissions`, and nothing else, as `get_my_permissions` answers them. */
export function holding(permissions: { resource: string; action: string }[]) {
  return http.get("*/api/access/me/permissions", () => HttpResponse.json({ permissions }));
}

// Added before each test, after `vitest.setup.ts` resets the last one's, so a test's own
// `server.use(holding(..))` takes precedence.
beforeEach(() => {
  server.use(holding(everyAreasNeeds));
});
