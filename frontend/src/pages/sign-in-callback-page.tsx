import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import { type SignInError, useSession } from "@/lib/sign-in";
import { useRouter } from "@tanstack/react-router";
import { useEffect, useState } from "react";

/** Where the provider sends the user back: completes the sign-in, then returns to where they started. */
export function SignInCallbackPage() {
  const session = useSession();
  const router = useRouter();
  // Where the provider came back to: returning to the user's page changes the location.
  const [href] = useState(() => router.state.location.href);
  const [error, setError] = useState<SignInError>();

  useEffect(() => {
    if (!session) {
      router.history.replace("/");
      return;
    }
    let current = true;
    session.completeSignIn(new URL(href, window.location.origin).href).then(
      (returnTo) => current && router.history.replace(returnTo),
      (failure: SignInError) => current && setError(failure),
    );
    return () => {
      current = false;
    };
  }, [session, router, href]);

  if (!error) return null;
  return (
    <main className="mx-auto max-w-4xl p-6">
      <Empty>
        <EmptyHeader>
          <EmptyTitle>
            <h1>Sign-in didn't complete</h1>
          </EmptyTitle>
          <EmptyDescription>{error.message}</EmptyDescription>
        </EmptyHeader>
        <EmptyContent>
          <Button onClick={() => session?.signIn(error.returnTo)}>Try again</Button>
        </EmptyContent>
      </Empty>
    </main>
  );
}
