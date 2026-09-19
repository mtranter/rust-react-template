import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { setupServer } from "msw/node";
import { afterAll, afterEach, beforeAll } from "vitest";
import { client } from "./src/api/client.gen";

// Shared MSW server. Tests add handlers with `server.use(...)`; mocking at the
// network boundary keeps the real generated client in the path (spec §13).
export const server = setupServer();

// The client's prod baseUrl is a relative "/api" (src/hey-api.ts). Node's fetch
// rejects relative URLs (browsers resolve them against the origin), so give the
// real client an absolute base for the test environment only.
beforeAll(() => {
  client.setConfig({ baseUrl: "http://localhost/api" });
  server.listen({ onUnhandledRequest: "error" });
});
afterEach(() => {
  server.resetHandlers();
  cleanup();
});
afterAll(() => server.close());
