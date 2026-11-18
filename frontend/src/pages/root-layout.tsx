import { ThemeToggle } from "@/components/theme-toggle";
import { useManagesAccess } from "@/features/access";
import { Link, Outlet } from "@tanstack/react-router";

export function RootLayout() {
  const managesAccess = useManagesAccess();
  return (
    <>
      <header className="mx-auto flex max-w-4xl items-center justify-end gap-4 px-6 pt-4">
        {managesAccess && (
          <Link to="/access" className="text-sm underline-offset-4 hover:underline">
            Access
          </Link>
        )}
        <ThemeToggle />
      </header>
      <Outlet />
    </>
  );
}
