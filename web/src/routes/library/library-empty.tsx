import { FilterXIcon, UploadCloudIcon } from "lucide-react";
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

/** The first-run state: a drop target that says what to do and what happens next. */
export function LibraryEmpty({ onUpload }: { onUpload: () => void }) {
  return (
    <section
      aria-label="Your library is empty"
      className="relative overflow-hidden rounded-xl border-2 border-dashed border-border-strong/40 bg-surface/60 px-6 py-14 text-center sm:py-20"
    >
      <div className="mx-auto flex max-w-lg flex-col items-center">
        <span className="grid size-14 place-items-center rounded-full border border-border bg-surface text-accent-text shadow-xs">
          <UploadCloudIcon className="size-6" strokeWidth={1.5} aria-hidden />
        </span>
        <h2 className="display mt-6 text-2xl text-fg sm:text-3xl">Drop files here to begin</h2>
        <p className="mt-3 text-sm text-fg-muted">
          Drag documents, scans, photos or notes anywhere on this page, or choose them from your
          device. Akasha reads each one, finds the text in scans, and makes every page searchable.
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
