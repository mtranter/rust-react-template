import { buttonVariants } from "@/components/ui/button";
import { productName } from "@/shell/capabilities";
import { Link } from "@tanstack/react-router";
import { LayoutGridIcon } from "lucide-react";

/**
 * Where signing out ends. It never leaves for the provider by itself: were the provider's own
 * session still alive, that would sign the user straight back in.
 */
export function SignedOutPage() {
  return (
    <main data-app="home" className="bg-sidebar grid min-h-svh place-items-center p-6">
      <div className="bg-background w-full max-w-sm space-y-6 rounded-2xl border-t-4 border-t-primary p-8 shadow-sm">
        <span className="bg-primary text-primary-foreground flex size-10 items-center justify-center rounded-lg">
          <LayoutGridIcon className="size-5" />
        </span>
        <div className="space-y-1">
          <h1 className="font-heading text-3xl font-bold tracking-tight">You're signed out</h1>
          <p className="text-muted-foreground text-sm">
            Sign in again to get back to {productName}.
          </p>
        </div>
        <Link to="/" className={buttonVariants({ className: "w-full" })}>
          Sign in again
        </Link>
      </div>
    </main>
  );
}
