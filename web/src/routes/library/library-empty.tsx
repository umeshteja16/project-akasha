import {
  FilterXIcon,
  HistoryIcon,
  MessageSquareQuoteIcon,
  SearchIcon,
  UploadCloudIcon,
} from "lucide-react";
import { EmptyState } from "@/components/common/empty-state";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { Skeleton } from "@/components/ui/skeleton";
import type { LibraryView } from "./use-library-prefs";

const FORMATS = [
  "PDF",
  "PNG · JPEG · WebP",
  "Markdown",
  "Plain text",
  "CSV · JSON",
  "Audio · Video",
];

const STEPS = [
  {
    icon: UploadCloudIcon,
    title: "Keep",
    text: "Drop in documents, scans, photos and notes. Akasha reads each one, even the text in pictures.",
  },
  {
    icon: SearchIcon,
    title: "Find",
    text: "Search by the words you remember or by what you meant. Results open at the passage.",
  },
  {
    icon: MessageSquareQuoteIcon,
    title: "Ask",
    text: "Ask a question and get an answer from your files, with every claim linked to its source.",
  },
] as const;

/**
 * First run: what Akasha does in three steps, and a drop target that says what to
 * do and what happens next.
 */
export function LibraryEmpty({ onUpload, name }: { onUpload: () => void; name?: string }) {
  return (
    <div className="grid gap-8">
      <section
        aria-labelledby="welcome-title"
        className="relative overflow-hidden rounded-xl border-2 border-dashed border-border-strong/40 bg-surface/60 px-6 py-12 text-center sm:py-16"
      >
        <div className="mx-auto flex max-w-lg flex-col items-center">
          <span className="grid size-14 place-items-center rounded-full border border-border bg-surface text-accent-text shadow-xs">
            <UploadCloudIcon className="size-6" strokeWidth={1.5} aria-hidden />
          </span>
          <p className="eyebrow mt-6">{name ? `Welcome, ${name}` : "Welcome"}</p>
          <h2 id="welcome-title" className="display mt-2 text-2xl text-fg sm:text-3xl">
            Drop your first files here
          </h2>
          <p className="mt-3 text-sm text-fg-muted">
            Drag documents, scans, photos or notes anywhere on this page, or choose them from your
            device. Each one becomes searchable within moments.
          </p>
          <div className="mt-6 flex flex-wrap items-center justify-center gap-3">
            <Button size="lg" onClick={onUpload}>
              Choose files
            </Button>
            <span className="text-xs text-fg-subtle">
              or press <Kbd>U</Kbd>
            </span>
          </div>
          <ul aria-label="Supported formats" className="mt-8 flex flex-wrap justify-center gap-1.5">
            {FORMATS.map((f) => (
              <li
                key={f}
                className="rounded-full border border-border bg-surface px-2.5 py-0.5 font-mono text-2xs text-fg-subtle"
              >
                {f}
              </li>
            ))}
          </ul>
        </div>
      </section>
      <section aria-labelledby="how-title" className="grid gap-4">
        <h2 id="how-title" className="eyebrow">
          How Akasha works
        </h2>
        <ol className="grid gap-6 sm:grid-cols-3 sm:gap-8">
          {STEPS.map((step, i) => (
            <li key={step.title} className="grid content-start gap-2 border-t border-border pt-4">
              <p className="flex items-center gap-2 text-fg">
                <span className="font-mono text-2xs text-fg-subtle">0{i + 1}</span>
                <step.icon className="size-4 text-accent-text" aria-hidden />
                <span className="display text-xl">{step.title}</span>
              </p>
              <p className="text-sm text-fg-muted">{step.text}</p>
            </li>
          ))}
        </ol>
        <p className="text-xs text-fg-subtle">
          Your files are stored and read on the server you signed in to. Settings → System shows
          which models it uses.
        </p>
      </section>
    </div>
  );
}

/** "Recently opened" with nothing opened yet. */
export function NothingOpened({ onShowAll }: { onShowAll: () => void }) {
  return (
    <EmptyState
      icon={HistoryIcon}
      title="Nothing opened yet"
      actions={
        <Button variant="secondary" onClick={onShowAll}>
          Show newest first
        </Button>
      }
    >
      <p>Files you open appear here, most recent first.</p>
    </EmptyState>
  );
}

export function NoMatches({ onClear }: { onClear: () => void }) {
  return (
    <EmptyState
      icon={FilterXIcon}
      title="Nothing matches these filters"
      actions={
        <Button variant="secondary" onClick={onClear}>
          Clear filters
        </Button>
      }
    >
      <p>Try another type or tag, or clear the filters to see everything.</p>
    </EmptyState>
  );
}

export function LibraryLoading({ view }: { view: LibraryView }) {
  const cells = Array.from({ length: 8 }, (_, i) => i);
  return (
    <div role="status" aria-busy="true" aria-label="Loading your library">
      {view === "grid" ? (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 sm:gap-4 lg:grid-cols-4">
          {cells.map((i) => (
            <div key={i} className="overflow-hidden rounded-lg border border-border bg-surface">
              <Skeleton className="aspect-[4/3] rounded-none" />
              <div className="grid gap-2 p-3">
                <Skeleton className="h-4 w-4/5" />
                <Skeleton className="h-3 w-1/2" />
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="divide-y divide-border rounded-lg border border-border bg-surface">
          {cells.map((i) => (
            <div key={i} className="flex items-center gap-3 px-4 py-3">
              <Skeleton className="size-10" />
              <Skeleton className="h-4 flex-1" />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
