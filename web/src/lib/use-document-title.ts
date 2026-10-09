import { useEffect } from "react";

/** "Library · Akasha": the tab title follows the screen (and what is open on it). */
export function useDocumentTitle(title: string | null | undefined) {
  useEffect(() => {
    document.title = title ? `${title} · Akasha` : "Akasha";
  }, [title]);
}
