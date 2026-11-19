import { buttonVariants } from "@/components/ui/button";
import { Empty, EmptyContent, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Link } from "@tanstack/react-router";

/**
 * Where signing out ends. It never leaves for the provider by itself: were the provider's own
 * session still alive, that would sign the user straight back in.
 */
export function SignedOutPage() {
  return (
    <main className="mx-auto max-w-4xl p-6">
      <Empty>
        <EmptyHeader>
          <EmptyTitle>
            <h1>You're signed out</h1>
          </EmptyTitle>
        </EmptyHeader>
        <EmptyContent>
          <Link to="/" className={buttonVariants()}>
            Sign in again
          </Link>
        </EmptyContent>
      </Empty>
    </main>
  );
}
