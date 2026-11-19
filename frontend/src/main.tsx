import { UserManager } from "oidc-client-ts";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { Session, fetchSignInConfig, userManagerSettings } from "./lib/sign-in";
import "./index.css";

// Importing the generated client applies the "/api" runtime config (src/hey-api.ts).
import "./api/client.gen";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root element");

async function signIn(): Promise<Session | null> {
  const config = await fetchSignInConfig();
  return config && Session.restore(new UserManager(userManagerSettings(config, location.origin)));
}

signIn().then((session) =>
  createRoot(root).render(
    <StrictMode>
      <App session={session} />
    </StrictMode>,
  ),
);
