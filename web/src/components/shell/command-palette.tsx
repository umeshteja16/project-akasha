import {
  createContext,
  lazy,
  type ReactNode,
  Suspense,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";

// The palette (cmdk and its dialog) loads on first use, not with the app.
const CommandPalette = lazy(() =>
  import("./command-palette-dialog").then((m) => ({ default: m.CommandPalette })),
);

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
  const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    if (isOpen) setLoaded(true);
  }, [isOpen]);

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
      {loaded || isOpen ? (
        <Suspense fallback={null}>
          <CommandPalette open={isOpen} onOpenChange={setOpen} />
        </Suspense>
      ) : null}
    </PaletteContext.Provider>
  );
}
