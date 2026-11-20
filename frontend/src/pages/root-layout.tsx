import { SignedInUser } from "@/components/signed-in-user";
import { ThemeToggle } from "@/components/theme-toggle";
import { useManagesAccess } from "@/features/access";
import { useAssistantOn } from "@/features/assistant";
import { RequireSignIn } from "@/lib/sign-in";
import { Link, Outlet } from "@tanstack/react-router";

export function RootLayout() {
  return (
    <RequireSignIn>
      <Header />
      <Outlet />
    </RequireSignIn>
  );
}

function Header() {
  const managesAccess = useManagesAccess();
  const assistantOn = useAssistantOn();
  return (
    <header className="mx-auto flex max-w-4xl items-center justify-end gap-4 px-6 pt-4">
      {assistantOn && (
        <Link to="/assistant" className="text-sm underline-offset-4 hover:underline">
          Assistant
        </Link>
      )}
      {managesAccess && (
        <Link to="/access" className="text-sm underline-offset-4 hover:underline">
          Access
        </Link>
      )}
      <SignedInUser />
      <ThemeToggle />
    </header>
  );
}
