import { createContext, useContext } from "react";
import type { Session } from "./session";

/** `null` when the app signs no one in (no `/auth/config`, as in `make dev`). */
export const SessionContext = createContext<Session | null>(null);

export function useSession(): Session | null {
  return useContext(SessionContext);
}
