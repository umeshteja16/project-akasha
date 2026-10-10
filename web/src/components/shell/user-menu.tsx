import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  ChevronsUpDownIcon,
  KeyboardIcon,
  LogOutIcon,
  PaletteIcon,
  Settings2Icon,
} from "lucide-react";
import { useApi } from "@/api/context";
import { meQuery } from "@/api/queries";
import { Avatar } from "@/components/common/avatar";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { type ThemePreference, useTheme } from "@/lib/theme";
import { cn } from "@/lib/utils";
import { MORE_ITEMS } from "./nav-items";
import { openShortcuts } from "./shortcuts-store";
import { useSignOut } from "./use-sign-out";

/** Avatar menu: settings, theme, sign out. `compact` shows only the avatar. */
export function UserMenu({ compact = false }: { compact?: boolean }) {
  const api = useApi();
  const { data: me } = useQuery(meQuery(api));
  const { preference, setPreference } = useTheme();
  const signOut = useSignOut();
  if (!me) return null;
  const name = me.display_name || me.email;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label="Account menu"
        className={cn(
          "flex items-center gap-2.5 rounded-md text-left transition-colors hover:bg-surface-2",
          compact ? "p-1" : "w-full p-2",
        )}
      >
        <Avatar name={name} />
        {compact ? null : (
          <>
            <span className="grid min-w-0 flex-1">
              <span className="truncate text-sm font-medium text-fg">{name}</span>
              {me.display_name ? (
                <span className="truncate text-xs text-fg-subtle">{me.email}</span>
              ) : null}
            </span>
            <ChevronsUpDownIcon className="size-4 text-fg-subtle" />
          </>
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align={compact ? "end" : "start"}
        side={compact ? "bottom" : "top"}
        className="w-60"
      >
        <DropdownMenuLabel>
          <span className="block truncate text-sm font-medium text-fg">{name}</span>
          <span className="block truncate text-xs text-fg-subtle">{me.email}</span>
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        {compact
          ? MORE_ITEMS.map((item) => (
              <DropdownMenuItem key={item.to} asChild>
                <Link to={item.to}>
                  <item.icon />
                  {item.label}
                </Link>
              </DropdownMenuItem>
            ))
          : null}
        <DropdownMenuItem asChild>
          <Link to="/settings">
            <Settings2Icon />
            Settings
          </Link>
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={openShortcuts}>
          <KeyboardIcon />
          Keyboard shortcuts
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuLabel className="flex items-center gap-2 eyebrow">
          <PaletteIcon className="size-3" /> Theme
        </DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={preference}
          onValueChange={(value) => setPreference(value as ThemePreference)}
        >
          <DropdownMenuRadioItem value="system">System</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="light">Light</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="dark">Dark</DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => signOut.mutate()} disabled={signOut.isPending}>
          <LogOutIcon />
          Sign out
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
