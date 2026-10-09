import { ArrowDownIcon } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";

/** How close to the end counts as "at the latest message". */
const NEAR_END = 96;

/**
 * The scrolling message column. It follows new content while the reader is at the
 * end, stays put once they scroll up (offering "Jump to latest"), and jumps to the
 * end whenever `followKey` changes (a new question was sent).
 */
export function ChatScroll({ children, followKey }: { children: ReactNode; followKey: unknown }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const follow = useRef(true);
  const [atEnd, setAtEnd] = useState(true);

  const toEnd = useCallback((smooth = false) => {
    const el = scrollRef.current;
    if (!el) return;
    follow.current = true;
    setAtEnd(true);
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    el.scrollTo({ top: el.scrollHeight, behavior: smooth && !reduce ? "smooth" : "auto" });
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: follow when the key changes
  useLayoutEffect(() => {
    toEnd();
  }, [followKey, toEnd]);

  useEffect(() => {
    const el = scrollRef.current;
    const content = contentRef.current;
    if (!el || !content || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (follow.current) el.scrollTop = el.scrollHeight;
      else setAtEnd(el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_END);
    });
    observer.observe(content);
    return () => observer.disconnect();
  }, []);

  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={scrollRef}
        className="h-full overflow-y-auto overscroll-contain"
        onScroll={(e) => {
          const el = e.currentTarget;
          const near = el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_END;
          follow.current = near;
          setAtEnd(near);
        }}
      >
        <div ref={contentRef}>{children}</div>
      </div>
      {atEnd ? null : (
        <div className="pointer-events-none absolute inset-x-0 bottom-3 flex justify-center">
          <Button
            variant="secondary"
            size="sm"
            className="pointer-events-auto animate-fade-in rounded-full shadow-md"
            onClick={() => toEnd(true)}
          >
            <ArrowDownIcon /> Jump to latest
          </Button>
        </div>
      )}
    </div>
  );
}
