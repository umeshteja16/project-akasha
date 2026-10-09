import { ChevronRightIcon } from "lucide-react";
import type { FileResults, Timings } from "@/api/search";

const STAGES: ReadonlyArray<[keyof Timings, string]> = [
  ["embed_ms", "Embed query"],
  ["keyword_ms", "Keyword"],
  ["semantic_ms", "Semantic"],
  ["rerank_ms", "Rerank"],
  ["fetch_ms", "Snippets"],
];

const ms = (n: number) => (n < 10 ? n.toFixed(1) : Math.round(n).toString());

/** "How this search ran": modes, reranking, warnings and per-stage timings. */
export function SearchDebug({ data }: { data: FileResults }) {
  const total = Math.max(data.timings.total_ms, 0.001);
  return (
    <details className="group rounded-lg border border-border text-xs text-fg-muted open:bg-surface-2/40">
      <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-2 text-fg-subtle select-none hover:text-fg-muted [&::-webkit-details-marker]:hidden">
        <ChevronRightIcon
          className="size-3.5 transition-transform group-open:rotate-90"
          aria-hidden
        />
        How this search ran
        <span className="ml-auto font-mono">{ms(data.timings.total_ms)} ms</span>
      </summary>
      <div className="grid gap-3 border-t border-border px-3 py-3">
        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1">
          <dt className="text-fg-subtle">Mode</dt>
          <dd className="font-mono">
            {data.mode}
            {data.mode !== data.requested_mode ? ` (asked for ${data.requested_mode})` : ""}
          </dd>
          <dt className="text-fg-subtle">Reranked</dt>
          <dd className="font-mono">{data.reranked ? "yes" : "no"}</dd>
          {data.warnings.length ? (
            <>
              <dt className="text-fg-subtle">Warnings</dt>
              <dd>{data.warnings.join(" · ")}</dd>
            </>
          ) : null}
        </dl>
        <ul className="grid gap-1.5" aria-label="Time per stage">
          {STAGES.map(([key, label]) => {
            const value = data.timings[key];
            return (
              <li
                key={key}
                className="grid grid-cols-[6rem_minmax(0,1fr)_3.5rem] items-center gap-3"
              >
                <span className="text-fg-subtle">{label}</span>
                <span className="h-1.5 overflow-hidden rounded-full bg-surface-3">
                  <span
                    className="block h-full rounded-full bg-accent/70"
                    style={{ width: `${Math.min(100, (value / total) * 100)}%` }}
                  />
                </span>
                <span className="text-right font-mono">{ms(value)} ms</span>
              </li>
            );
          })}
        </ul>
      </div>
    </details>
  );
}
