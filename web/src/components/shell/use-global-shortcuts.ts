import { useNavigate } from "@tanstack/react-router";
import { useEffect } from "react";
import { useUploader } from "@/features/upload/upload-context";
import { NAV_ITEMS } from "./nav-items";

/** True while the person is typing somewhere (shortcuts must not steal keys). */
export function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

/**
 * App-wide single-key shortcuts (outside text fields, without modifiers):
 * `/` focuses the page's search field (or opens Search), `u` opens the file
 * picker, `g` then a letter goes to a screen (g l, g s, g c, g ,).
 */
export function useGlobalShortcuts() {
  const navigate = useNavigate();
  const { pick } = useUploader();

  useEffect(() => {
    let pendingG = 0;
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
      if (isTyping(event.target)) return;
      if (document.querySelector("[role=dialog]")) return;

      if (pendingG && Date.now() - pendingG < 1200) {
        pendingG = 0;
        const item = NAV_ITEMS.find((i) => i.key === event.key);
        if (item) {
          event.preventDefault();
          void navigate({ to: item.to });
        }
        return;
      }
      if (event.key === "g") {
        pendingG = Date.now();
        return;
      }
      if (event.key === "/") {
        event.preventDefault();
        const field = document.querySelector<HTMLElement>("[data-search-field]");
        if (field) field.focus();
        else void navigate({ to: "/search" });
        return;
      }
      if (event.key === "u") {
        event.preventDefault();
        pick();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate, pick]);
}
