import { RequireSignIn } from "@/lib/sign-in";
import { AppShell } from "@/shell/app-shell";

export function RootLayout() {
  return (
    <RequireSignIn>
      <AppShell />
    </RequireSignIn>
  );
}
