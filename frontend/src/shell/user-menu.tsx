import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { signedOutPath, useSession } from "@/lib/sign-in";
import { useNavigate } from "@tanstack/react-router";
import { LogOutIcon } from "lucide-react";

/** Who is signed in, from their ID token, and a way out; nothing when the app signs no one in. */
export function UserMenu() {
  const session = useSession();
  const navigate = useNavigate();
  const profile = session?.user?.profile;
  if (!session || !profile) return null;

  const name = profile.name ?? profile.email ?? profile.sub;
  const signOut = async () => {
    if (!(await session.signOut())) await navigate({ to: signedOutPath });
  };
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={<Button variant="ghost" size="sm" aria-label={`Account: ${name}`} />}
      >
        <Avatar size="sm">
          <AvatarFallback className="text-[0.625rem]">{initials(name)}</AvatarFallback>
        </Avatar>
        <span className="hidden max-w-40 truncate lg:inline">{name}</span>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-52">
        <DropdownMenuGroup>
          <DropdownMenuLabel className="text-foreground grid">
            <span className="truncate">{name}</span>
            {profile.email && profile.email !== name && (
              <span className="text-muted-foreground truncate text-xs font-normal">
                {profile.email}
              </span>
            )}
          </DropdownMenuLabel>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => void signOut()}>
          <LogOutIcon />
          Sign out
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function initials(name: string) {
  const words = name.split(/[\s@._-]+/).filter(Boolean);
  return words
    .slice(0, 2)
    .map((word) => word[0]?.toUpperCase())
    .join("");
}
