import { Link } from "@tanstack/react-router";
import { SearchIcon } from "lucide-react";
import { Wordmark } from "@/components/common/wordmark";
import { Kbd } from "@/components/ui/kbd";
import { isApple } from "@/lib/utils";
import { useCommandPalette } from "./command-palette";
import { NAV_ITEMS } from "./nav-items";
import { ThemeToggle } from "./theme-toggle";
import { UserMenu } from "./user-menu";

export function Sidebar() {
  const palette = useCommandPalette();
  return (
    <div className="hidden border-r border-border bg-sidebar md:block">
      <aside className="sticky top-0 flex h-dvh flex-col">
        <div className="flex h-16 items-center justify-between px-5">
          <Link to="/library" aria-label="Akasha home" className="rounded-sm">
            <Wordmark />
          </Link>
          <ThemeToggle />
        </div>

        <div className="px-3">
          <button
            type="button"
            onClick={palette.open}
            className="flex h-9 w-full items-center gap-2 rounded-md border border-border bg-surface px-3 text-left text-sm text-fg-subtle shadow-xs transition-colors hover:border-border-strong hover:text-fg-muted"
          >
            <SearchIcon className="size-4" aria-hidden />
            <span className="flex-1">Jump to…</span>
            <span className="flex gap-0.5">
              <Kbd>{isApple() ? "⌘" : "Ctrl"}</Kbd>
              <Kbd>K</Kbd>
            </span>
          </button>
        </div>

        <nav aria-label="Main" className="mt-6 grid gap-0.5 px-3">
          <p className="eyebrow px-3 pb-2">Memory</p>
          {NAV_ITEMS.map((item) => (
            <Link
              key={item.to}
              to={item.to}
              className="group relative flex h-9 items-center gap-3 rounded-md px-3 text-sm text-fg-muted transition-colors hover:bg-surface-2 hover:text-fg data-[status=active]:bg-surface data-[status=active]:font-medium data-[status=active]:text-fg data-[status=active]:shadow-xs"
            >
              <span
                aria-hidden
                className="absolute top-2 bottom-2 left-0 w-0.5 rounded-full bg-accent opacity-0 transition-opacity group-data-[status=active]:opacity-100"
              />
              <item.icon className="size-4 text-fg-subtle group-data-[status=active]:text-accent-text" />
              {item.label}
            </Link>
          ))}
        </nav>

        <div className="mt-auto border-t border-border p-3">
          <UserMenu />
        </div>
      </aside>
    </div>
  );
}
