import { useCallback, useState } from "react";
import type { FileSort } from "@/api/files";

export type LibraryView = "grid" | "list";

export interface LibraryPrefs {
  view: LibraryView;
  sort: FileSort;
}

const KEY = "akasha-library";
const DEFAULTS: LibraryPrefs = { view: "grid", sort: "newest" };
const SORTS: readonly FileSort[] = ["newest", "oldest", "name", "size"];

export function readPrefs(): LibraryPrefs {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<Record<keyof LibraryPrefs, unknown>>;
    return {
      view: parsed.view === "list" ? "list" : "grid",
      sort: SORTS.find((s) => s === parsed.sort) ?? DEFAULTS.sort,
    };
  } catch {
    return DEFAULTS;
  }
}

/** View (grid/list) and sort order, remembered per browser. */
export function useLibraryPrefs() {
  const [prefs, setPrefs] = useState<LibraryPrefs>(readPrefs);
  const update = useCallback((changes: Partial<LibraryPrefs>) => {
    setPrefs((old) => {
      const next = { ...old, ...changes };
      try {
        window.localStorage.setItem(KEY, JSON.stringify(next));
      } catch {
        // Private mode or storage full: the choice lasts for this visit only.
      }
      return next;
    });
  }, []);
  return [prefs, update] as const;
}
