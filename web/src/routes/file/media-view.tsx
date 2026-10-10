import { useInfiniteQuery } from "@tanstack/react-query";
import { AudioLinesIcon } from "lucide-react";
import { type ReactNode, useEffect, useId, useMemo, useRef, useState } from "react";
import { useApi } from "@/api/context";
import { downloadUrl, extractionQuery, type FileDetail } from "@/api/files";
import { EmptyState } from "@/components/common/empty-state";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";
import { FileThumb } from "@/features/files/file-thumb";
import { categoryOf } from "@/features/files/kind";
import { formatTimestamp, type Passage } from "@/features/files/passage";
import { cn } from "@/lib/utils";
import { lineAt, linesInPassage, transcriptLines } from "./transcript-lines";

/** Transcripts are loaded whole, up to this many 50k-character windows (~16 hours of speech). */
const MAX_WINDOWS = 20;
const SEEK_SLACK_MS = 300;

/**
 * A recording: its player and its transcript. Every line has a timestamp that
 * plays from there; the line being spoken is highlighted. `startAt` (seconds, from
 * a `?t=` link) moves the player there, and the linked passage is marked.
 */
export default function MediaView({
  file,
  startAt,
  passage,
}: {
  file: FileDetail;
  startAt?: number;
  passage: Passage | null;
}) {
  const api = useApi();
  const ready = file.status === "ready";
  const query = useInfiniteQuery(extractionQuery(api, file.id, ready));
  const playerRef = useRef<HTMLMediaElement | null>(null);
  const listRef = useRef<HTMLOListElement | null>(null);
  const [nowMs, setNowMs] = useState((startAt ?? 0) * 1000);

  const titleId = useId();
  const windows = useMemo(() => query.data?.pages.filter((p) => p !== null) ?? [], [query.data]);
  const first = windows[0];
  useEffect(() => {
    if (query.hasNextPage && !query.isFetchingNextPage && windows.length < MAX_WINDOWS) {
      void query.fetchNextPage();
    }
  }, [query.hasNextPage, query.isFetchingNextPage, windows.length, query.fetchNextPage]);

  const lines = useMemo(
    () => (first?.extractor === "transcript" ? transcriptLines(windows, first.segments) : []),
    [windows, first],
  );
  // Players land a little before the requested time (they seek to a frame).
  const active = lineAt(lines, nowMs + SEEK_SLACK_MS);
  const marked = useMemo(() => linesInPassage(lines, passage), [lines, passage]);

  // A `?t=` link: move the player there once it knows the duration (unless the
  // listener already picked a time).
  const userSeeked = useRef(false);
  useEffect(() => {
    const el = playerRef.current;
    if (!el || startAt == null) return;
    const go = () => {
      if (!userSeeked.current) el.currentTime = startAt;
    };
    if (el.readyState >= 1) go();
    else el.addEventListener("loadedmetadata", go, { once: true });
    return () => el.removeEventListener("loadedmetadata", go);
  }, [startAt]);

  // Bring the linked line into view once the transcript is there.
  const target =
    startAt != null ? lineAt(lines, startAt * 1000) : (marked.values().next().value ?? -1);
  const scrolled = useRef(false);
  useEffect(() => {
    if (scrolled.current || target < 0) return;
    const el = listRef.current?.querySelector<HTMLElement>(`[data-line="${target}"]`);
    if (!el) return;
    scrolled.current = true;
    el.scrollIntoView({ block: "nearest" });
  }, [target]);

  const seek = (ms: number) => {
    const el = playerRef.current;
    if (!el) return;
    userSeeked.current = true;
    el.currentTime = ms / 1000;
    setNowMs(ms);
    void el.play().catch(() => {});
  };
  const onTime = () => {
    const el = playerRef.current;
    if (el) setNowMs(el.currentTime * 1000);
  };
  const src = downloadUrl(file.id);
  const frame = "overflow-hidden rounded-lg border border-border bg-surface shadow-xs";

  return (
    <div className="grid gap-5">
      {categoryOf(file.mime_type) === "video" ? (
        <div className={`${frame} bg-black`}>
          {/* biome-ignore lint/a11y/useMediaCaption: the transcript below is the text */}
          <video
            ref={(el) => {
              playerRef.current = el;
            }}
            controls
            preload="metadata"
            src={src}
            onTimeUpdate={onTime}
            className="max-h-[60vh] w-full"
          >
            Your browser can't play this video.
          </video>
        </div>
      ) : (
        <div className={`${frame} grid gap-4 p-5`}>
          <FileThumb
            id={file.id}
            mime={file.mime_type}
            status={file.status}
            className="aspect-[5/1] rounded-md"
          />
          {/* biome-ignore lint/a11y/useMediaCaption: the transcript below is the text */}
          <audio
            ref={(el) => {
              playerRef.current = el;
            }}
            controls
            preload="metadata"
            src={src}
            onTimeUpdate={onTime}
            className="w-full"
          >
            Your browser can't play this audio.
          </audio>
        </div>
      )}

      <section aria-labelledby={titleId} className="grid gap-3">
        <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
          <h2 id={titleId} className="eyebrow">
            Transcript
          </h2>
          {first?.duration_ms ? (
            <span className="font-mono text-2xs text-fg-subtle">
              {formatTimestamp(first.duration_ms)} transcribed
            </span>
          ) : null}
        </div>
        <TranscriptBody
          file={file}
          loading={ready && query.isPending}
          failed={query.isError}
          notes={first?.notes ?? []}
          empty={lines.length === 0}
        >
          <ol
            ref={listRef}
            className="grid max-h-[60vh] gap-0.5 overflow-y-auto rounded-lg border border-border bg-surface p-2 shadow-xs sm:p-3"
          >
            {lines.map((line, i) => (
              <li
                key={line.charStart}
                data-line={i}
                aria-current={i === active ? "true" : undefined}
                className={cn(
                  "grid grid-cols-[4.5rem_minmax(0,1fr)] items-baseline gap-2 rounded-md px-2 py-1.5",
                  i === active && "bg-accent-soft",
                )}
              >
                <button
                  type="button"
                  onClick={() => seek(line.startMs)}
                  aria-label={`Play from ${formatTimestamp(line.startMs)}`}
                  className="w-fit rounded-xs font-mono text-xs text-accent-text tabular-nums hover:underline"
                >
                  {formatTimestamp(line.startMs)}
                </button>
                <span
                  className={cn(
                    "font-display text-[0.97rem] leading-relaxed text-fg-muted [overflow-wrap:anywhere]",
                    i === active && "text-fg",
                    marked.has(i) && "rounded-xs bg-mark text-fg shadow-[0_0_0_3px_var(--mark)]",
                  )}
                  data-passage={marked.has(i) ? "" : undefined}
                >
                  {line.text}
                </span>
              </li>
            ))}
          </ol>
        </TranscriptBody>
      </section>
    </div>
  );
}

function TranscriptBody({
  file,
  loading,
  failed,
  notes,
  empty,
  children,
}: {
  file: FileDetail;
  loading: boolean;
  failed: boolean;
  notes: string[];
  empty: boolean;
  children: ReactNode;
}) {
  if (file.status === "pending" || file.status === "processing") {
    const progress = file.processing?.progress;
    return (
      <div className="grid gap-2 rounded-lg border border-border bg-surface px-4 py-4 shadow-xs">
        <p className="text-sm text-fg-muted">
          Listening to this recording…{" "}
          {progress != null
            ? `${Math.round(progress * 100)}% done.`
            : "The transcript appears here when it's ready."}
        </p>
        {progress != null ? <Progress value={progress} label="Transcription progress" /> : null}
      </div>
    );
  }
  if (file.status === "failed") {
    return (
      <EmptyState icon={AudioLinesIcon} title="No transcript">
        <p>The recording couldn't be transcribed. Retry to try again.</p>
      </EmptyState>
    );
  }
  if (loading) {
    return (
      <div className="grid gap-2" role="status" aria-busy="true" aria-label="Loading transcript">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-4" style={{ width: `${85 - i * 15}%` }} />
        ))}
      </div>
    );
  }
  if (failed) return <p className="text-sm text-danger">Couldn't load the transcript.</p>;
  return (
    <>
      {notes.length ? (
        <p className="rounded-md bg-surface-2 px-3 py-2 text-xs text-fg-muted">{notes.join(" ")}</p>
      ) : null}
      {empty ? null : children}
    </>
  );
}
