import { ThemeLoader } from "@/lib/appearance";
import { RequireSignIn } from "@/lib/sign-in";
import { AppShell } from "@/shell/app-shell";

export function RootLayout() {
  return (
    <>
      <ThemeLoader />
      <RequireSignIn>
        <AppShell />
      </RequireSignIn>
    </>
  );
}
