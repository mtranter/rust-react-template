import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { setupServer } from "msw/node";
import { afterAll, afterEach, beforeAll } from "vitest";
import { client } from "./src/api/client.gen";

// Every screen's layout asks what the caller may do and whether the assistant is on: unless a test
// says otherwise, they may use every Area (`src/test/may-use-everything.ts`, a setup file of its
// own as it reads the shell's registry), and it is off, as with no model configured. The catalog
// answers only an admin, so a test of one answers it itself (`accessApi`).
export const server = setupServer(
  http.get("*/api/assistant/status", () => HttpResponse.json({ on: false })),
  // No app hidden.
  http.get("*/api/appearance/apps", () => HttpResponse.json({ version: 0, hidden: [] })),
  // No custom theme: the defaults in `theme.css`.
  http.get("*/api/appearance/theme", () =>
    HttpResponse.json({ version: 0, light: {}, dark: {}, shared: {} }),
  ),
  http.get("*/api/access/catalog", () =>
    HttpResponse.json(
      { code: "forbidden", message: "forbidden", requestId: "test" },
      { status: 403 },
    ),
  ),
);

// `renderAt` renders the whole app; on a loaded machine its first screen can take past `findBy`'s 1s.
configure({ asyncUtilTimeout: 5_000 });

// jsdom doesn't implement scrolling; the router scrolls to top on navigation.
Object.assign(globalThis, { scrollTo: () => {} });

// jsdom has no matchMedia, which the shell's sidebar asks to tell a phone; nothing matches.
if (typeof (globalThis as { matchMedia?: unknown }).matchMedia !== "function") {
  Object.assign(globalThis, {
    matchMedia: (media: string) => ({
      media,
      matches: false,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

// jsdom has no ResizeObserver or scrollIntoView, which the Go to palette's list (cmdk) uses.
const { Element } = globalThis as unknown as {
  Element: { prototype: { scrollIntoView?: () => void } };
};
Element.prototype.scrollIntoView ??= () => {};
if (!("ResizeObserver" in globalThis)) {
  Object.assign(globalThis, {
    ResizeObserver: class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  });
}

// jsdom has no PointerEvent, which Base UI's checkbox dispatches when clicked.
if (!("PointerEvent" in globalThis)) {
  const { MouseEvent } = globalThis as unknown as {
    MouseEvent: new (type: string, init?: object) => object;
  };
  class PointerEvent extends MouseEvent {
    readonly pointerId: number;
    readonly pointerType: string;
    constructor(type: string, init: { pointerId?: number; pointerType?: string } = {}) {
      super(type, init);
      this.pointerId = init.pointerId ?? 0;
      this.pointerType = init.pointerType ?? "mouse";
    }
  }
  Object.assign(globalThis, { PointerEvent });
}

// Node's fetch rejects the client's relative "/api" baseUrl (browsers resolve it
// against the origin), so tests use an absolute one.
beforeAll(() => {
  client.setConfig({ baseUrl: "http://localhost/api" });
  server.listen({ onUnhandledRequest: "error" });
});
afterEach(() => {
  server.resetHandlers();
  cleanup();
  // Sign-in state (oidc-client-ts' and the `Session`'s) lives in the tab's `sessionStorage`.
  sessionStorage.clear();
  // Per-viewer conveniences (the theme, whether the assistant's panel is open).
  localStorage.clear();
});
afterAll(() => server.close());
