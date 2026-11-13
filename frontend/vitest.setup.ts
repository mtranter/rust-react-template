import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { setupServer } from "msw/node";
import { afterAll, afterEach, beforeAll } from "vitest";
import { client } from "./src/api/client.gen";

export const server = setupServer();

// `renderAt` renders the whole app; on a loaded machine its first screen can take past `findBy`'s 1s.
configure({ asyncUtilTimeout: 5_000 });

// jsdom doesn't implement scrolling; the router scrolls to top on navigation.
Object.assign(globalThis, { scrollTo: () => {} });

// Node's fetch rejects the client's relative "/api" baseUrl (browsers resolve it
// against the origin), so tests use an absolute one.
beforeAll(() => {
  client.setConfig({ baseUrl: "http://localhost/api" });
  server.listen({ onUnhandledRequest: "error" });
});
afterEach(() => {
  server.resetHandlers();
  cleanup();
});
afterAll(() => server.close());
