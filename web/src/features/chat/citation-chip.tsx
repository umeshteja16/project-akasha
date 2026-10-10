import { Link, useNavigate } from "@tanstack/react-router";
import { ArrowUpRightIcon } from "lucide-react";
import { useRef, useState } from "react";
import type { Citation } from "@/api/chat";
import { Popover, PopoverAnchor, PopoverContent } from "@/components/ui/popover";
import { kindOf } from "@/features/files/kind";
import { formatTimestamp, locationLabel, passageSearch } from "@/features/files/passage";

/** Link target of a citation: the file, opened at the cited passage and page. */
export function citationLink(c: Citation) {
  return {
    to: "/files/$fileId" as const,
    params: { fileId: c.file_id },
    search: passageSearch(c.char_start, c.char_end, c.page, c.start_ms),
  };
}

const HOVER_DELAY = 120;

/** ", page 3" / ", at 1:05" for screen readers. */
function spokenLocation(c: Citation): string {
  if (c.page) return `, page ${c.page}`;
  if (c.start_ms != null) return `, at ${formatTimestamp(c.start_ms)}`;
  return "";
}

function openLabel(c: Citation): string {
  if (c.page) return `Open at page ${c.page}`;
  if (c.start_ms != null) return `Play from ${formatTimestamp(c.start_ms)}`;
  return "Open the passage";
}

/**
 * An inline `[n]`. Mouse: hover shows the quote, click opens the file there.
 * Touch and keyboard: tap/Enter shows the quote, with a link to open the file.
 */
export function CitationChip({ citation }: { citation: Citation }) {
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const pointer = useRef<string>("");
  const timer = useRef<number | undefined>(undefined);
  const hover = (next: boolean) => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setOpen(next), next ? HOVER_DELAY : HOVER_DELAY * 2);
  };
  const Icon = kindOf(fileMime(citation.file_name)).icon;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverAnchor asChild>
        <button
          type="button"
          data-citation={citation.n}
          aria-label={`Source ${citation.n}: ${citation.file_name}${spokenLocation(citation)}`}
          aria-expanded={open}
          onPointerDown={(e) => {
            pointer.current = e.pointerType;
          }}
          onPointerEnter={(e) => {
            if (e.pointerType === "mouse") hover(true);
          }}
          onPointerLeave={(e) => {
            if (e.pointerType === "mouse") hover(false);
          }}
          onClick={() => {
            if (pointer.current === "mouse") {
              setOpen(false);
              void navigate(citationLink(citation));
            } else {
              setOpen((v) => !v);
            }
            pointer.current = "";
          }}
          className="relative -top-px mx-0.5 inline-flex h-[1.15rem] min-w-[1.15rem] items-center justify-center rounded-sm border border-accent/30 bg-accent-soft px-1 align-baseline font-mono text-[0.68rem] leading-none font-medium text-accent-text transition-colors hover:border-accent/60 hover:bg-accent hover:text-accent-fg aria-expanded:border-accent/60"
        >
          {citation.n}
        </button>
      </PopoverAnchor>
      <PopoverContent
        side="top"
        className="grid w-80 gap-2.5 p-3.5"
        onOpenAutoFocus={(e) => {
          if (pointer.current === "mouse") e.preventDefault();
        }}
        onPointerEnter={() => window.clearTimeout(timer.current)}
        onPointerLeave={(e) => {
          if (e.pointerType === "mouse") hover(false);
        }}
      >
        <p className="flex items-center gap-2 text-xs text-fg-muted">
          <span className="grid size-5 shrink-0 place-items-center rounded-sm bg-accent-soft font-mono text-2xs text-accent-text">
            {citation.n}
          </span>
          <Icon className="size-3.5 shrink-0 text-fg-subtle" aria-hidden />
          <span className="min-w-0 flex-1 truncate font-medium text-fg">{citation.file_name}</span>
          {locationLabel(citation) ? (
            <span className="shrink-0 font-mono text-2xs text-fg-subtle uppercase">
              {locationLabel(citation)}
            </span>
          ) : null}
        </p>
        <blockquote className="line-clamp-6 border-l-2 border-border-strong/50 pl-3 font-display text-sm leading-relaxed text-fg">
          {citation.quote}
        </blockquote>
        <Link
          {...citationLink(citation)}
          onClick={() => setOpen(false)}
          className="inline-flex w-fit items-center gap-1 text-xs font-medium text-accent-text hover:underline"
        >
          {openLabel(citation)}
          <ArrowUpRightIcon className="size-3" aria-hidden />
        </Link>
      </PopoverContent>
    </Popover>
  );
}

/** A guess at the type from the name, for the icon only. */
function fileMime(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  if (ext === "pdf") return "application/pdf";
  if (ext === "md" || ext === "markdown") return "text/markdown";
  if (["png", "jpg", "jpeg", "gif", "webp", "heic", "tiff"].includes(ext)) return `image/${ext}`;
  return "text/plain";
}
