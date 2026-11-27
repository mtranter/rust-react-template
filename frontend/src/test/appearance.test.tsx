import { clearTheme } from "@/lib/appearance";
import { notAnAdmin } from "@/test/access-fixtures";
import { appearanceApi } from "@/test/appearance-fixtures";
import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test } from "vitest";
import { server } from "../../vitest.setup";

const style = () => document.getElementById("app-theme")?.textContent ?? "";

afterEach(() => clearTheme());

async function openEditor(api = appearanceApi()) {
  server.use(...api.handlers);
  const user = userEvent.setup();
  const app = renderAt("/appearance");
  await screen.findByRole("tab", { name: "Light" });
  return { user, api, ...app };
}

test("an edit previews across the app before it is saved", async () => {
  const { user, api } = await openEditor();

  await user.type(screen.getByLabelText("Primary"), "red");
  await waitFor(() => expect(style()).toContain("--primary:red;"));
  expect(api.saves).toHaveLength(0);

  await user.click(screen.getByRole("button", { name: "Discard changes" }));
  await waitFor(() => expect(style()).not.toContain("--primary"));
  expect(screen.getByLabelText("Primary")).toHaveValue("");
});

test("saves with If-Match and keeps the saved theme applied", async () => {
  const { user, api, queryClient } = await openEditor(appearanceApi({ version: 4 }));

  await user.type(screen.getByLabelText("Primary"), "red");
  await user.click(screen.getByRole("tab", { name: "Dark" }));
  await user.type(screen.getByLabelText("Primary"), "blue");
  await user.click(screen.getByRole("button", { name: "Save" }));

  await waitFor(() => expect(api.theme.current.version).toBe(5));
  expect(api.saves[0]).toMatchObject({ light: { primary: "red" }, dark: { primary: "blue" } });
  expect(api.saves[0]?.shared).toEqual({});
  await waitFor(() =>
    expect(queryClient.getQueryData(["appearance", "theme"])).toMatchObject({ version: 5 }),
  );
  expect(style()).toBe(":root:not(.dark){--primary:red;}.dark.dark{--primary:blue;}");
  expect(localStorage.getItem("app-theme-css")).toBe(style());
});

test("the Light/Dark tabs are the app's own light/dark choice, both ways", async () => {
  const { user } = await openEditor(appearanceApi({ version: 1 }));
  expect(screen.getByRole("tab", { name: "Light" })).toHaveAttribute("aria-selected", "true");

  await user.click(screen.getByRole("tab", { name: "Dark" }));
  expect(document.documentElement).toHaveClass("dark");
  expect(localStorage.getItem("app-theme")).toBe("dark");

  await user.click(screen.getByRole("button", { name: "Theme" }));
  await user.click(await screen.findByRole("menuitemradio", { name: "Light" }));
  expect(document.documentElement).not.toHaveClass("dark");
  expect(screen.getByRole("tab", { name: "Light" })).toHaveAttribute("aria-selected", "true");
  expect(screen.queryByRole("switch", { name: "Preview dark" })).not.toBeInTheDocument();
});

test("shows a server field error on its input", async () => {
  const { user } = await openEditor();
  await user.type(screen.getByLabelText("Primary"), "red;x");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByText("must not contain `;`")).toBeInTheDocument();
});

test("someone else's save meanwhile reloads, keeps the edit, and the retry saves", async () => {
  const api = appearanceApi({ version: 1 });
  const { user } = await openEditor(api);
  api.theme.current = { ...api.theme.current, version: 2, light: { ring: "green" } };

  await user.type(screen.getByLabelText("Primary"), "red");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByText(/Someone else changed this theme/)).toBeInTheDocument();
  expect(screen.getByLabelText("Primary")).toHaveValue("red");
  expect(api.saves).toHaveLength(0);

  await user.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(api.theme.current.version).toBe(3));
  expect(api.saves[0]?.light).toEqual({ primary: "red" });
});

test("resets to the defaults after confirming", async () => {
  const api = appearanceApi({ version: 2, light: { primary: "red" }, shared: { radius: "0rem" } });
  const { user } = await openEditor(api);
  expect(screen.getByLabelText("Primary")).toHaveValue("red");

  await user.click(screen.getByRole("button", { name: "Reset to defaults" }));
  const dialog = await screen.findByRole("alertdialog");
  await user.click(within(dialog).getByRole("button", { name: "Reset" }));

  await waitFor(() => expect(api.theme.current.version).toBe(3));
  expect(api.saves[0]).toEqual({ light: {}, dark: {}, shared: {} });
  await waitFor(() => expect(screen.getByLabelText("Primary")).toHaveValue(""));
  expect(style()).toBe("");
  await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
});

test("a caller who may not change it sees a refusal and no Admin entry for it", async () => {
  server.use(...notAnAdmin);
  const { queryClient } = renderAt("/appearance");

  expect(await screen.findByText(/You may not change the appearance/)).toBeInTheDocument();
  await waitFor(() => expect(queryClient.isFetching()).toBe(0));
  expect(screen.queryByRole("tab", { name: "Light" })).not.toBeInTheDocument();
  expect(screen.queryByRole("link", { name: /Appearance/ })).not.toBeInTheDocument();
  expect(screen.queryByRole("link", { name: /Admin/ })).not.toBeInTheDocument();
});
