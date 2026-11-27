import type { AppVisibility, AppVisibilityInput, ErrorBody } from "@/api";
import { notAnAdmin } from "@/test/access-fixtures";
import { notesApi } from "@/test/fixtures";
import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

// Over the reference Notes app, so `make init` removes it with Notes.

/** Which apps are hidden, as the server answers: versioned with `If-Match`. */
function visibilityApi(hidden: string[] = [], version = 1) {
  const state: { current: AppVisibility } = { current: { version, hidden } };
  const saves: AppVisibilityInput[] = [];
  server.use(
    http.get("*/api/appearance/apps", () => HttpResponse.json(state.current)),
    http.put("*/api/appearance/apps", async ({ request }) => {
      if (request.headers.get("If-Match") !== `"${state.current.version}"`) {
        return HttpResponse.json<ErrorBody>(
          { code: "precondition_failed", message: "stale", requestId: "test" },
          { status: 412 },
        );
      }
      const body = (await request.json()) as AppVisibilityInput;
      saves.push(body);
      state.current = { version: state.current.version + 1, hidden: body.hidden };
      return HttpResponse.json(state.current);
    }),
  );
  return { state, saves };
}

test("a hidden app leaves the launcher, though its URL still routes", async () => {
  visibilityApi(["notes"]);
  server.use(...notesApi([]).handlers);
  const { queryClient, router } = renderAt("/");

  await waitFor(() => expect(queryClient.isFetching()).toBe(0));
  expect(screen.queryByRole("region", { name: "Apps" })).not.toBeInTheDocument();

  await router.navigate({ to: "/notes" });
  expect(await screen.findByRole("heading", { name: "Notes" })).toBeInTheDocument();
});

test("shows no app until the server says which are hidden", async () => {
  server.use(http.get("*/api/appearance/apps", () => new Promise<never>(() => {})));
  renderAt("/");

  await new Promise((resolve) => setTimeout(resolve, 100));
  expect(screen.queryByRole("region", { name: "Apps" })).not.toBeInTheDocument();
  expect(screen.queryByRole("region", { name: "Platform" })).not.toBeInTheDocument();
});

test("an admin hides an app with If-Match, and it leaves the launcher", async () => {
  const api = visibilityApi([], 3);
  const user = userEvent.setup();
  const { router } = renderAt("/appearance/apps");

  const shown = await screen.findByRole("switch", { name: "Show Notes" });
  await waitFor(() => expect(shown).not.toBeDisabled());
  expect(shown).toBeChecked();
  await user.click(shown);

  await waitFor(() => expect(api.state.current.version).toBe(4));
  expect(api.saves).toEqual([{ hidden: ["notes"] }]);
  await waitFor(() => expect(shown).not.toBeChecked());

  await router.navigate({ to: "/" });
  expect(await screen.findByRole("region", { name: "Platform" })).toBeInTheDocument();
  expect(screen.queryByRole("region", { name: "Apps" })).not.toBeInTheDocument();
});

test("a change made meanwhile is reloaded and said, not overwritten", async () => {
  const api = visibilityApi([], 1);
  const user = userEvent.setup();
  renderAt("/appearance/apps");
  const shown = await screen.findByRole("switch", { name: "Show Notes" });
  await waitFor(() => expect(shown).not.toBeDisabled());

  api.state.current = { version: 2, hidden: ["notes"] };
  await user.click(shown);

  expect(await screen.findByText(/Someone else changed this list of apps/)).toBeInTheDocument();
  await waitFor(() => expect(shown).not.toBeChecked());
  expect(api.saves).toEqual([]);
});

test("a caller who isn't an admin has no Apps area", async () => {
  server.use(...notAnAdmin);
  const { queryClient } = renderAt("/");

  await waitFor(() => expect(queryClient.isFetching()).toBe(0));
  const apps = await screen.findByRole("region", { name: "Apps" });
  expect(within(apps).getByRole("link", { name: /Notes/ })).toBeInTheDocument();
  expect(screen.queryByRole("region", { name: "Platform" })).not.toBeInTheDocument();
});
