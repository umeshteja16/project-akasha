import { Link } from "@tanstack/react-router";
import { SearchIcon } from "lucide-react";
import { Wordmark } from "@/components/common/wordmark";
import { Button } from "@/components/ui/button";
import { useCommandPalette } from "./command-palette";
import { NAV_ITEMS } from "./nav-items";
import { ThemeToggle } from "./theme-toggle";
import { UserMenu } from "./user-menu";

/** Phone/tablet top bar. */
export function MobileHeader() {
  const palette = useCommandPalette();
  return (
    <header className="sticky top-0 z-30 flex h-14 items-center justify-between border-b border-border bg-bg/90 px-4 backdrop-blur-md md:hidden">
      <Link to="/library" aria-label="Akasha home">
        <Wordmark />
      </Link>
      <div className="flex items-center gap-1">
        <Button variant="ghost" size="icon-sm" onClick={palette.open} aria-label="Jump to">
          <SearchIcon />
        </Button>
        <ThemeToggle />
        <UserMenu compact />
      </div>
    </header>
  );
}

/** Phone/tablet bottom tab bar (thumb reach). */
export function MobileTabBar() {
  return (
    <nav
      aria-label="Main"
      className="fixed inset-x-0 bottom-0 z-30 grid grid-cols-4 border-t border-border bg-bg/95 pb-[env(safe-area-inset-bottom)] backdrop-blur-md md:hidden"
    >
      {NAV_ITEMS.map((item) => (
        <Link
          key={item.to}
          to={item.to}
          className="group flex h-16 flex-col items-center justify-center gap-1 text-2xs font-medium text-fg-subtle data-[status=active]:text-accent-text"
        >
          <span className="grid h-7 w-12 place-items-center rounded-full transition-colors group-data-[status=active]:bg-accent-soft">
            <item.icon className="size-[18px]" />
          </span>
          {item.label}
        </Link>
      ))}
    </nav>
  );
}
