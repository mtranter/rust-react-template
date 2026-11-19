import { useLocation } from "@tanstack/react-router";
import { type ReactNode, useEffect, useState } from "react";
import { useSession } from "./context";

/**
 * Renders `children` to a signed-in user, or to anyone when the app signs no one in; anyone else
 * leaves straight for the provider's sign-in page, to come back here.
 */
export function RequireSignIn({ children }: { children: ReactNode }) {
  const session = useSession();
  const returnTo = useLocation({ select: (location) => location.href });
  // Read once: signing out empties the session, and leaving for the provider then would sign
  // the user straight back in.
  const [signedIn] = useState(() => !session || session.user !== null);
  useEffect(() => {
    if (session && !signedIn) void session.signIn(returnTo);
  }, [session, signedIn, returnTo]);
  return signedIn ? children : null;
}
