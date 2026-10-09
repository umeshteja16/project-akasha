import { useNavigate } from "@tanstack/react-router";
import { Command } from "cmdk";
import {
  LogOutIcon,
  MonitorIcon,
  MoonIcon,
  SearchIcon,
  SunIcon,
  SwatchBookIcon,
} from "lucide-react";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";
import { useTheme } from "@/lib/theme";
import { isApple } from "@/lib/utils";
import { NAV_ITEMS } from "./nav-items";
import { useSignOut } from "./use-sign-out";

interface PaletteContextValue {
  open: () => void;
}

const PaletteContext = createContext<PaletteContextValue | null>(null);

export function useCommandPalette(): PaletteContextValue {
  const ctx = useContext(PaletteContext);
  if (!ctx) throw new Error("useCommandPalette must be used inside <CommandPaletteProvider>");
  return ctx;
}

/** ⌘K / Ctrl+K anywhere opens the palette. Search results join it with the search screen. */
export function CommandPaletteProvider({ children }: { children: ReactNode }) {
  const [isOpen, setOpen] = useState(false);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() === "k" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        setOpen((open) => !open);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const value = useMemo(() => ({ open: () => setOpen(true) }), []);
  return (
    <PaletteContext.Provider value={value}>
      {children}
      <CommandPalette open={isOpen} onOpenChange={setOpen} />
    </PaletteContext.Provider>
  );
}

const itemClass =
  "flex cursor-default items-center gap-3 rounded-md px-3 py-2 text-sm text-fg select-none data-[selected=true]:bg-surface-2 [&_svg]:size-4 [&_svg]:text-fg-subtle";
const groupClass =
  "[&_[cmdk-group-heading]]:eyebrow [&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:pt-3 [&_[cmdk-group-heading]]:pb-1.5";

function CommandPalette({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const navigate = useNavigate();
  const { setPreference } = useTheme();
  const signOut = useSignOut();

  const run = useCallback(
    (action: () => void) => {
      onOpenChange(false);
      action();
    },
    [onOpenChange],
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent hideClose className="max-w-xl gap-0 overflow-hidden p-0">
        <DialogTitle className="sr-only">Command palette</DialogTitle>
        <DialogDescription className="sr-only">
          Jump to a screen or run a command. Search inside your files arrives with the search
          screen.
        </DialogDescription>
        <Command label="Command palette" className="flex flex-col">
          <div className="flex items-center gap-3 border-b border-border px-4">
            <SearchIcon className="size-4 text-fg-subtle" aria-hidden />
            <Command.Input
              autoFocus
              placeholder="Jump to…"
              className="h-13 flex-1 bg-transparent text-base text-fg outline-none placeholder:text-fg-subtle"
            />
            <Kbd>esc</Kbd>
          </div>
          <Command.List className="max-h-[min(60vh,24rem)] overflow-y-auto p-2">
            <Command.Empty className="px-3 py-8 text-center text-sm text-fg-muted">
              Nothing matches. Full-text search of your files is coming to this box.
            </Command.Empty>
            <Command.Group heading="Go to" className={groupClass}>
              {NAV_ITEMS.map((item) => (
                <Command.Item
                  key={item.to}
                  value={`go ${item.label}`}
                  onSelect={() => run(() => void navigate({ to: item.to }))}
                  className={itemClass}
                >
                  <item.icon />
                  {item.label}
                </Command.Item>
              ))}
              <Command.Item
                value="go design system"
                onSelect={() => run(() => void navigate({ to: "/design" }))}
                className={itemClass}
              >
                <SwatchBookIcon />
                Design system
              </Command.Item>
            </Command.Group>
            <Command.Group heading="Theme" className={groupClass}>
              <Command.Item
                onSelect={() => run(() => setPreference("light"))}
                className={itemClass}
              >
                <SunIcon /> Light theme
              </Command.Item>
              <Command.Item onSelect={() => run(() => setPreference("dark"))} className={itemClass}>
                <MoonIcon /> Dark theme
              </Command.Item>
              <Command.Item
                onSelect={() => run(() => setPreference("system"))}
                className={itemClass}
              >
                <MonitorIcon /> Match system theme
              </Command.Item>
            </Command.Group>
            <Command.Group heading="Account" className={groupClass}>
              <Command.Item onSelect={() => run(() => signOut.mutate())} className={itemClass}>
                <LogOutIcon /> Sign out
              </Command.Item>
            </Command.Group>
          </Command.List>
          <div className="flex items-center justify-between border-t border-border bg-surface-2/60 px-4 py-2 text-xs text-fg-subtle">
            <span className="flex items-center gap-1.5">
              <Kbd>↑</Kbd>
              <Kbd>↓</Kbd> to move <Kbd>↵</Kbd> to open
            </span>
            <span className="flex items-center gap-1">
              <Kbd>{isApple() ? "⌘" : "Ctrl"}</Kbd>
              <Kbd>K</Kbd>
            </span>
          </div>
        </Command>
      </DialogContent>
    </Dialog>
  );
}
