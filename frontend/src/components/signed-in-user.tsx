import { Button } from "@/components/ui/button";
import { signedOutPath, useSession } from "@/lib/sign-in";
import { useNavigate } from "@tanstack/react-router";

/** Who is signed in, from their ID token, and a way out; nothing when the app signs no one in. */
export function SignedInUser() {
  const session = useSession();
  const navigate = useNavigate();
  const profile = session?.user?.profile;
  if (!session || !profile) return null;

  const signOut = async () => {
    if (!(await session.signOut())) await navigate({ to: signedOutPath });
  };
  return (
    <>
      <span className="text-sm text-muted-foreground">
        {profile.name ?? profile.email ?? profile.sub}
      </span>
      <Button variant="ghost" size="sm" onClick={signOut}>
        Sign out
      </Button>
    </>
  );
}
