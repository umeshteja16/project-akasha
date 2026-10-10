import { Link } from "@tanstack/react-router";
import { ChevronRightIcon } from "lucide-react";
import { useId, useState } from "react";
import type { Citation } from "@/api/chat";
import { locationLabel } from "@/features/files/passage";
import { citationLink } from "./citation-chip";

/** The passages an answer cites, numbered like the chips in the text. */
export function SourcesPanel({
  citations,
  defaultOpen = false,
}: {
  citations: Citation[];
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const id = useId();
  if (citations.length === 0) return null;
  const files = new Set(citations.map((c) => c.file_id)).size;
  return (
    <div className="grid gap-2">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((v) => !v)}
        className="inline-flex w-fit items-center gap-1.5 rounded-sm text-xs font-medium text-fg-muted hover:text-fg"
      >
        <ChevronRightIcon
          className={`size-3.5 transition-transform ${open ? "rotate-90" : ""}`}
          aria-hidden
        />
        {citations.length} {citations.length === 1 ? "source" : "sources"}
        <span className="font-normal text-fg-subtle">
          from {files} {files === 1 ? "file" : "files"}
        </span>
      </button>
      {open ? (
        <ol id={id} aria-label="Sources" className="grid animate-fade-in gap-1">
          {citations.map((c) => (
            <li key={`${c.n}-${c.chunk_id}`}>
              <Link
                {...citationLink(c)}
                className="group grid grid-cols-[1.5rem_minmax(0,1fr)] gap-3 rounded-md px-2 py-2 transition-colors hover:bg-surface-2"
              >
                <span className="mt-0.5 grid size-5 place-items-center rounded-sm bg-accent-soft font-mono text-2xs text-accent-text">
                  {c.n}
                </span>
                <span className="grid min-w-0 gap-0.5">
                  <span className="flex items-baseline gap-2 text-sm">
                    <span className="truncate font-medium text-fg group-hover:underline">
                      {c.file_name}
                    </span>
                    {locationLabel(c) ? (
                      <span className="shrink-0 font-mono text-2xs text-fg-subtle uppercase">
                        {locationLabel(c)}
                      </span>
                    ) : null}
                  </span>
                  <span className="line-clamp-2 font-display text-sm text-fg-muted">{c.quote}</span>
                </span>
              </Link>
            </li>
          ))}
        </ol>
      ) : null}
    </div>
  );
}
