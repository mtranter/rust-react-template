import { ThemeToggle } from "@/components/theme-toggle";
import { Outlet } from "@tanstack/react-router";

export function RootLayout() {
  return (
    <>
      <header className="mx-auto flex max-w-4xl justify-end px-6 pt-4">
        <ThemeToggle />
      </header>
      <Outlet />
    </>
  );
}
