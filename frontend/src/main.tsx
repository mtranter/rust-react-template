import { UserManager } from "oidc-client-ts";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { Session, fetchSignInConfig, isSilentRenewal, userManagerSettings } from "./lib/sign-in";
import "./index.css";

// Importing the generated client applies the "/api" runtime config (src/hey-api.ts).
import "./api/client.gen";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root element");

const start = async () => {
  const config = await fetchSignInConfig();
  const provider = config && new UserManager(userManagerSettings(config, location.origin));
  if (provider && isSilentRenewal()) return provider.signinSilentCallback();
  const session = provider && (await Session.restore(provider));
  createRoot(root).render(
    <StrictMode>
      <App session={session} />
    </StrictMode>,
  );
};

void start();
