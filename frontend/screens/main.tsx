import { App } from "@/App";
import { createAppRouter } from "@/router";
import { createMemoryHistory } from "@tanstack/react-router";
import { setupWorker } from "msw/browser";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { scenarios } from "./scenarios";
import "@/index.css";

// Importing the generated client applies the "/api" runtime config (src/hey-api.ts).
import "@/api/client.gen";

const name = new URLSearchParams(window.location.search).get("scenario");
const scenario = scenarios.find((s) => s.name === name);
if (!scenario) throw new Error(`no scenario named "${name}"`);

await setupWorker(...scenario.handlers).start({
  serviceWorker: { url: "/screens/mockServiceWorker.js" },
  quiet: true,
  onUnhandledRequest: (request, print) => {
    if (new URL(request.url).pathname.startsWith("/api/")) print.error();
  },
});

const router = createAppRouter(createMemoryHistory({ initialEntries: [scenario.route] }));

const root = document.getElementById("root");
if (!root) throw new Error("missing #root element");

createRoot(root).render(
  <StrictMode>
    <App router={router} />
  </StrictMode>,
);
