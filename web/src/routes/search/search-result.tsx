import { Link } from "@tanstack/react-router";
import { PinIcon, SparklesIcon } from "lucide-react";
import type { FileHit } from "@/api/search";
import { Highlighted } from "@/components/common/highlighted";
import { FileThumb } from "@/features/files/file-thumb";
import { kindOf } from "@/features/files/kind";
import { formatTimestamp, locationLabel, passageSearch } from "@/features/files/passage";
import { formatRelative } from "@/lib/format";

/** Distinct page numbers of the matches, in order ("p. 3, 7"); for recordings
 *  the times ("1:05, 12:40"). */
export function pagesLabel(hit: FileHit): string | null {
  const pages = [...new Set(hit.matches.map((m) => m.page).filter((p): p is number => p != null))];
  if (pages.length === 0) {
    const times = [
      ...new Set(hit.matches.map((m) => m.start_ms).filter((t): t is number => t != null)),
    ];
    if (times.length === 0) return null;
    return times
      .sort((a, b) => a - b)
      .map(formatTimestamp)
      .join(", ");
  }
  return `${pages.length === 1 ? "p." : "pp."} ${pages.sort((a, b) => a - b).join(", ")}`;
}

/** One file with its best passages; each passage links to that place in the file. */
export function SearchResult({ hit }: { hit: FileHit }) {
  const { file } = hit;
  const kind = kindOf(file.mime_type);
  const best = hit.matches[0];
  const pages = pagesLabel(hit);
  const tags = [...file.tags, ...file.auto_tags.filter((t) => !file.tags.includes(t))].slice(0, 4);
  const extra = hit.match_count - hit.matches.length;

  return (
    <article className="grid grid-cols-[2.5rem_minmax(0,1fr)] gap-x-4 border-b border-border py-6 first:pt-2 last:border-b-0 sm:grid-cols-[3rem_minmax(0,1fr)]">
      <Link
        to="/files/$fileId"
        params={{ fileId: file.id }}
        tabIndex={-1}
        aria-hidden
        className="block self-start overflow-hidden rounded-md border border-border shadow-xs"
      >
        <FileThumb
          id={file.id}
          mime={file.mime_type}
          status={file.status}
          variant="icon"
          className="size-10 rounded-none sm:size-12"
        />
      </Link>
      <div className="grid min-w-0 gap-2">
        <header className="grid gap-0.5">
          <h3 className="min-w-0 text-base font-semibold text-fg">
            <Link
              to="/files/$fileId"
              params={{ fileId: file.id }}
              search={
                best ? passageSearch(best.char_start, best.char_end, best.page, best.start_ms) : {}
              }
              className="break-words decoration-accent/50 underline-offset-4 hover:underline"
            >
              {file.name}
            </Link>
          </h3>
          <p className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-fg-subtle">
            <span>{kind.label}</span>
            {pages ? (
              <>
                <Dot />
                <span className="font-mono">{pages}</span>
              </>
            ) : null}
            <Dot />
            <span>
              {hit.match_count} {hit.match_count === 1 ? "passage" : "passages"}
            </span>
            <Dot />
            <time dateTime={file.created_at}>added {formatRelative(file.created_at)}</time>
            {file.is_pinned ? (
              <>
                <Dot />
                <span className="inline-flex items-center gap-1 text-accent-text">
                  <PinIcon className="size-3 fill-current" aria-hidden /> Pinned
                </span>
              </>
            ) : null}
          </p>
        </header>

        {file.summary ? (
          <p className="line-clamp-2 max-w-[var(--reading-max)] text-sm text-fg-muted">
            {file.summary}
          </p>
        ) : null}

        {hit.matches.length ? (
          <ol className="grid gap-1" aria-label={`Passages from ${file.name}`}>
            {hit.matches.map((match) => (
              <li key={match.chunk_id}>
                <Link
                  to="/files/$fileId"
                  params={{ fileId: file.id }}
                  search={passageSearch(
                    match.char_start,
                    match.char_end,
                    match.page,
                    match.start_ms,
                  )}
                  className="group/passage -mx-3 grid grid-cols-[auto_minmax(0,1fr)] gap-3 rounded-md px-3 py-2 transition-colors hover:bg-surface-2"
                >
                  <span
                    className="mt-1 h-[calc(100%-0.5rem)] w-0.5 rounded-full bg-border transition-colors group-hover/passage:bg-accent"
                    aria-hidden
                  />
                  <span className="min-w-0">
                    {locationLabel(match) ? (
                      <span className="mr-2 font-mono text-2xs tracking-wide text-fg-subtle uppercase">
                        {locationLabel(match)}
                      </span>
                    ) : null}
                    <span className="font-display text-[0.97rem] leading-relaxed text-fg-muted [overflow-wrap:anywhere]">
                      <Highlighted text={match.snippet.text} spans={match.snippet.highlights} />
                    </span>
                  </span>
                </Link>
              </li>
            ))}
          </ol>
        ) : null}

        {tags.length || extra > 0 ? (
          <p className="flex flex-wrap items-center gap-1.5 text-2xs text-fg-subtle">
            {tags.map((tag) => (
              <span
                key={tag}
                className="inline-flex items-center gap-1 rounded-full border border-border px-2 py-0.5"
              >
                {file.tags.includes(tag) ? null : (
                  <SparklesIcon className="size-2.5" aria-label="suggested" />
                )}
                {tag}
              </span>
            ))}
            {extra > 0 ? (
              <span className="ml-1">
                +{extra} more {extra === 1 ? "passage" : "passages"} in this file
              </span>
            ) : null}
          </p>
        ) : null}
      </div>
    </article>
  );
}

function Dot() {
  return (
    <span aria-hidden className="text-border-strong">
      ·
    </span>
  );
}
