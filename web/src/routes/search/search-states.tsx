import { Link } from "@tanstack/react-router";
import { CircleAlertIcon, SearchIcon, SearchXIcon, TimerIcon, ZapOffIcon } from "lucide-react";
import type { ReactNode } from "react";
import type { FileResults } from "@/api/search";
import { EmptyState } from "@/components/common/empty-state";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

const EXAMPLES = ["when does the lease renew", "notes on attention", "receipt from Lisbon"];

/** Before the first search: what works, with examples to try. */
export function SearchIntro({ onExample }: { onExample: (q: string) => void }) {
  return (
    <EmptyState icon={SearchIcon} title="Ask in your own words">
      <p>Exact phrases, half-remembered ideas, a name from a scanned receipt: all of it works.</p>
      <ul className="mt-5 flex flex-wrap justify-center gap-2" aria-label="Example searches">
        {EXAMPLES.map((example) => (
          <li key={example}>
            <button
              type="button"
              onClick={() => onExample(example)}
              className="rounded-full border border-border bg-surface px-3 py-1 font-mono text-xs text-fg-muted transition-colors hover:border-border-strong hover:text-fg"
            >
              {example}
            </button>
          </li>
        ))}
      </ul>
      <p className="mt-6 text-xs text-fg-subtle">
        Tip: press <kbd className="font-mono">/</kbd> anywhere to search, or{" "}
        <Link to="/chat" className="text-accent-text hover:underline">
          ask a question in Chat
        </Link>{" "}
        for an answer with sources.
      </p>
    </EmptyState>
  );
}

export function NoResults({
  query,
  filtered,
  mode,
  onClearFilters,
  onHybrid,
}: {
  query: string;
  filtered: boolean;
  mode: FileResults["mode"];
  onClearFilters: () => void;
  onHybrid: () => void;
}) {
  return (
    <EmptyState icon={SearchXIcon} title={`Nothing matched “${query}”`}>
      <ul className="mx-auto grid max-w-sm gap-1.5 text-left">
        <Hint>Check the spelling, or try fewer or more general words.</Hint>
        {mode === "keyword" ? (
          <Hint>
            Keyword search needs every word to appear.{" "}
            <Button variant="link" size="sm" className="h-auto text-sm" onClick={onHybrid}>
              Search by meaning too
            </Button>
          </Hint>
        ) : null}
        {filtered ? (
          <Hint>
            Filters are narrowing the search.{" "}
            <Button variant="link" size="sm" className="h-auto text-sm" onClick={onClearFilters}>
              Clear filters
            </Button>
          </Hint>
        ) : null}
        <Hint>
          Files still being read aren't searchable yet. Check the{" "}
          <Link to="/library" className="text-accent-text hover:underline">
            library
          </Link>
          .
        </Hint>
      </ul>
    </EmptyState>
  );
}

function Hint({ children }: { children: ReactNode }) {
  return (
    <li className="flex gap-2">
      <span className="mt-2 size-1 shrink-0 rounded-full bg-border-strong" aria-hidden />
      <span>{children}</span>
    </li>
  );
}

export function ResultsSkeleton() {
  return (
    <div className="grid gap-8 pt-2" role="status" aria-busy="true" aria-label="Searching">
      {[0, 1, 2].map((i) => (
        <div key={i} className="grid grid-cols-[3rem_minmax(0,1fr)] gap-4">
          <Skeleton className="size-12" />
          <div className="grid gap-2">
            <Skeleton className="h-4 w-1/3" />
            <Skeleton className="h-3 w-1/4" />
            <Skeleton className="h-4 w-11/12" />
            <Skeleton className="h-4 w-4/5" />
          </div>
        </div>
      ))}
    </div>
  );
}

type NoticeTone = "info" | "warning" | "danger";

export function Notice({
  tone = "info",
  children,
  action,
}: {
  tone?: NoticeTone;
  children: ReactNode;
  action?: ReactNode;
}) {
  const Icon = tone === "danger" ? CircleAlertIcon : tone === "warning" ? ZapOffIcon : TimerIcon;
  return (
    <div
      role={tone === "danger" ? "alert" : "status"}
      className={
        tone === "danger"
          ? "flex items-start gap-3 rounded-lg border border-danger/30 bg-danger-soft px-4 py-3 text-sm text-fg"
          : "flex items-start gap-3 rounded-lg border border-border bg-surface-2/60 px-4 py-3 text-sm text-fg"
      }
    >
      <Icon
        className={`mt-0.5 size-4 shrink-0 ${tone === "danger" ? "text-danger" : tone === "warning" ? "text-warning" : "text-fg-subtle"}`}
        aria-hidden
      />
      <div className="flex-1">{children}</div>
      {action}
    </div>
  );
}
