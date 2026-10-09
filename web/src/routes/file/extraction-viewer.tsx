import { useInfiniteQuery } from "@tanstack/react-query";
import { ScanTextIcon } from "lucide-react";
import { Fragment } from "react";
import { useApi } from "@/api/context";
import { extractionQuery, type FileItem } from "@/api/files";
import { EmptyState } from "@/components/common/empty-state";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { segmentByPages } from "./extraction-segments";

const SOURCE_NOTES: Record<string, string> = {
  ocr: "read from the scan",
  needs_ocr: "scanned page, OCR is off",
  unreadable: "no readable text",
};

const nf = new Intl.NumberFormat();

/** The text Akasha extracted, with page markers for PDFs. */
export function ExtractionViewer({ file }: { file: FileItem }) {
  const api = useApi();
  const query = useInfiniteQuery(extractionQuery(api, file.id, file.status === "ready"));

  if (file.status !== "ready") {
    return (
      <EmptyState icon={ScanTextIcon} title={file.status === "failed" ? "No text" : "Reading…"}>
        <p>
          {file.status === "failed"
            ? "The text couldn't be extracted. Retry to read the file again."
            : "The extracted text appears here once the file is ready."}
        </p>
      </EmptyState>
    );
  }
  if (query.isPending) {
    return (
      <div className="grid gap-2" role="status" aria-busy="true" aria-label="Loading text">
        {[0, 1, 2, 3].map((i) => (
          <Skeleton key={i} className="h-4" style={{ width: `${90 - i * 12}%` }} />
        ))}
      </div>
    );
  }
  if (query.isError) return <p className="text-sm text-danger">Couldn't load the text.</p>;
  const windows = query.data.pages.filter((p) => p !== null);
  const first = windows[0];
  if (!first || first.char_count === 0) {
    return (
      <EmptyState icon={ScanTextIcon} title="No text found">
        <p>
          {first?.notes.length
            ? first.notes.join(" ")
            : "Nothing readable was found in this file. Images without text and media files look like this."}
        </p>
      </EmptyState>
    );
  }

  return (
    <div className="grid gap-4">
      <p className="flex flex-wrap gap-x-4 gap-y-1 font-mono text-2xs text-fg-subtle uppercase">
        <span>{nf.format(first.char_count)} characters</span>
        {first.page_count ? <span>{first.page_count} pages</span> : null}
        <span>{nf.format(first.chunk_count)} passages indexed</span>
        <span>via {first.extractor}</span>
      </p>
      {first.notes.length ? (
        <p className="rounded-md bg-surface-2 px-3 py-2 text-xs text-fg-muted">
          {first.notes.join(" ")}
        </p>
      ) : null}
      <article className="rounded-lg border border-border bg-surface px-5 py-5 shadow-xs sm:px-8 sm:py-7">
        <div className="mx-auto max-w-[var(--reading-max)] font-display text-base leading-[1.7] text-fg">
          {windows.map((win) => (
            <Fragment key={win.offset}>
              {segmentByPages(win.text, win.offset, win.pages).map((seg, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: segments of a window never reorder
                <section key={`${win.offset}-${i}`}>
                  {seg.page && seg.startsHere ? (
                    <div className="my-5 flex items-center gap-3 first:mt-0">
                      <h3 className="eyebrow">Page {seg.page}</h3>
                      {seg.source && SOURCE_NOTES[seg.source] ? (
                        <span className="text-2xs text-fg-subtle">{SOURCE_NOTES[seg.source]}</span>
                      ) : null}
                      <span className="h-px flex-1 bg-border" />
                    </div>
                  ) : null}
                  <p className="whitespace-pre-wrap [overflow-wrap:anywhere]">{seg.text}</p>
                </section>
              ))}
            </Fragment>
          ))}
        </div>
      </article>
      {query.hasNextPage ? (
        <div className="flex justify-center">
          <Button
            variant="secondary"
            onClick={() => void query.fetchNextPage()}
            disabled={query.isFetchingNextPage}
          >
            {query.isFetchingNextPage ? "Loading…" : "Show more text"}
          </Button>
        </div>
      ) : null}
    </div>
  );
}
