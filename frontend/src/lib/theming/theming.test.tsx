import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it } from "vitest";
import {
  ThemeLoader,
  applyTheme,
  clearTheme,
  endPreview,
  previewTheme,
  themeCacheKey,
  themeCss,
} from ".";
import { server } from "../../../vitest.setup";

const style = () => document.getElementById("app-theme")?.textContent;

afterEach(() => clearTheme());

describe("themeCss", () => {
  it("scopes light out of dark mode, shared to the root and dark to .dark", () => {
    expect(
      themeCss({ light: { primary: "red" }, dark: { primary: "blue" }, shared: { radius: "0" } }),
    ).toBe(":root:not(.dark){--primary:red;}:root:root{--radius:0;}.dark.dark{--primary:blue;}");
  });
  it("is empty for defaults and drops values that would break out of the rule", () => {
    expect(themeCss({ light: {}, dark: {}, shared: {} })).toBe("");
    expect(themeCss({ light: { a: "x;}body{" }, dark: {}, shared: {} })).toBe("");
  });
});

describe("applying", () => {
  it("writes the style last in head and caches it", () => {
    applyTheme({ light: { primary: "red" }, dark: {}, shared: {} });
    expect(document.head.lastElementChild?.id).toBe("app-theme");
    expect(style()).toBe(":root:not(.dark){--primary:red;}");
    expect(localStorage.getItem(themeCacheKey)).toBe(":root:not(.dark){--primary:red;}");
  });
  it("previews unsaved tokens and restores the saved ones", () => {
    applyTheme({ light: { primary: "red" }, dark: {}, shared: {} });
    previewTheme({ light: { primary: "green" }, dark: {}, shared: {} });
    expect(style()).toBe(":root:not(.dark){--primary:green;}");
    expect(localStorage.getItem(themeCacheKey)).toBe(":root:not(.dark){--primary:red;}");
    endPreview();
    expect(style()).toBe(":root:not(.dark){--primary:red;}");
  });
  it("clears the style and the cache", () => {
    applyTheme({ light: { primary: "red" }, dark: {}, shared: {} });
    clearTheme();
    expect(document.getElementById("app-theme")).toBeNull();
    expect(localStorage.getItem(themeCacheKey)).toBeNull();
  });
});

describe("ThemeLoader", () => {
  const renderLoader = () =>
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ThemeLoader />
      </QueryClientProvider>,
    );
  it("applies the served theme", async () => {
    server.use(
      http.get("*/api/ui-settings/theme", () =>
        HttpResponse.json({ version: 2, light: {}, dark: { background: "black" }, shared: {} }),
      ),
    );
    renderLoader();
    await waitFor(() => expect(style()).toBe(".dark.dark{--background:black;}"));
  });
  it("keeps what is applied when the fetch fails", async () => {
    applyTheme({ light: { primary: "red" }, dark: {}, shared: {} });
    server.use(http.get("*/api/ui-settings/theme", () => new HttpResponse(null, { status: 404 })));
    renderLoader();
    await new Promise((r) => setTimeout(r, 50));
    expect(style()).toBe(":root:not(.dark){--primary:red;}");
  });
});
