import { useQuery } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";
import { ChevronLeftIcon, ChevronRightIcon, LoaderIcon, SearchIcon, XIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { isApiError } from "@/api/client";
import { useApi } from "@/api/context";
import { RESULTS_PER_PAGE, retryAfterSeconds, searchQuery } from "@/api/search";
import { PageHeader } from "@/components/common/page-header";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  activeFilters,
  clearFilters,
  MAX_QUERY,
  type SearchParams,
  updateParams,
} from "@/features/search/search-params";
import { useDebouncedValue } from "@/lib/use-debounced";
import { cn } from "@/lib/utils";
import { SearchDebug } from "./search-debug";
import { SearchFilters } from "./search-filters";
import { SearchResult } from "./search-result";
import { NoResults, Notice, ResultsSkeleton, SearchIntro } from "./search-states";

const route = getRouteApi("/app/search");

/** Wait this long after the last keystroke before searching (the API is rate-limited). */
export const DEBOUNCE_MS = 300;

export function SearchPage() {
  const params = route.useSearch();
  const navigate = route.useNavigate();
  const api = useApi();
  const inputRef = useRef<HTMLInputElement>(null);
  const resultsRef = useRef<HTMLDivElement>(null);

  // The box is local state; the URL follows it once typing pauses (replacing the
  // history entry), and follows the URL when it changes elsewhere (back/forward).
  const [draft, setDraft] = useState(params.q ?? "");
  const typed = useDebouncedValue(draft.trim(), DEBOUNCE_MS);
  const sent = useRef(params.q ?? "");
  const urlQ = params.q ?? "";

  useEffect(() => {
    if (typed === sent.current) return;
    sent.current = typed;
    void navigate({
      search: (prev: SearchParams) => updateParams(prev, { q: typed || undefined }),
      replace: true,
    });
  }, [typed, navigate]);

  useEffect(() => {
    if (urlQ === sent.current) return;
    sent.current = urlQ;
    setDraft(urlQ);
  }, [urlQ]);

  const go = (next: SearchParams, replace = false) => {
    sent.current = next.q ?? "";
    if ((next.q ?? "") !== draft.trim()) setDraft(next.q ?? "");
    void navigate({ search: next, replace });
  };

  const query = useQuery(searchQuery(api, params));
  const data = query.data;
  const pending = Boolean(params.q) && (query.isFetching || draft.trim() !== urlQ);
  const rateLimited = isApiError(query.error) && query.error.code === "rate_limited";
  const wait = retryAfterSeconds(query.error);
  const page = params.page ?? 1;

  // Retry automatically once a rate limit has passed.
  useEffect(() => {
    if (!rateLimited) return;
    const timer = window.setTimeout(() => void query.refetch(), (wait ?? 5) * 1000);
    return () => window.clearTimeout(timer);
  }, [rateLimited, wait, query.refetch]);

  const toPage = (n: number) => {
    go(updateParams(params, { page: n }));
    resultsRef.current?.scrollIntoView({ block: "start" });
  };

  return (
    <div className="grid gap-6">
      <PageHeader
        eyebrow="Find"
        title="Search"
        description="Search by words or by meaning across every page you've kept. Results show the passage, not just the file."
      />

      <div className="grid gap-4">
        {/* biome-ignore lint/a11y/useSemanticElements: <search> is unknown to jsdom (tests) */}
        <form
          role="search"
          className="relative"
          onSubmit={(event) => {
            event.preventDefault();
            const q = draft.trim();
            if (q !== urlQ) go(updateParams(params, { q: q || undefined }));
            else if (q) void query.refetch();
          }}
        >
          <SearchIcon
            className="pointer-events-none absolute top-1/2 left-4 size-[18px] -translate-y-1/2 text-fg-subtle"
            aria-hidden
          />
          <Input
            ref={inputRef}
            data-search-field
            type="search"
            autoFocus
            maxLength={MAX_QUERY}
            value={draft}
            onChange={(e) => setDraft(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape" && draft) {
                e.preventDefault();
                setDraft("");
              }
            }}
            placeholder="Search your library"
            aria-label="Search your library"
            className="h-12 rounded-lg pr-20 pl-11 text-base [&::-webkit-search-cancel-button]:hidden"
          />
          <span className="absolute top-1/2 right-3 flex -translate-y-1/2 items-center gap-1">
            {pending ? (
              <LoaderIcon
                className="size-4 animate-spin text-fg-subtle motion-reduce:animate-none"
                aria-label="Searching"
              />
            ) : null}
            {draft ? (
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Clear search"
                onClick={() => {
                  setDraft("");
                  inputRef.current?.focus();
                }}
              >
                <XIcon />
              </Button>
            ) : null}
          </span>
        </form>
        <SearchFilters params={params} onChange={(next) => go(next)} />
      </div>

      <div
        ref={resultsRef}
        className="grid scroll-mt-6 gap-4"
        aria-live="polite"
        aria-busy={pending}
      >
        {!params.q ? (
          <SearchIntro onExample={(q) => go(updateParams(params, { q }))} />
        ) : (
          <>
            {rateLimited ? (
              <Notice tone="info">
                You're searching faster than this server allows.{" "}
                {wait ? `Results resume in about ${wait} s.` : "Results resume in a moment."}
              </Notice>
            ) : null}
            {query.isError && !rateLimited ? (
              <Notice
                tone="danger"
                action={
                  <Button variant="secondary" size="sm" onClick={() => void query.refetch()}>
                    Try again
                  </Button>
                }
              >
                {isApiError(query.error) && query.error.status === 400
                  ? query.error.message
                  : "The search didn't go through."}
              </Notice>
            ) : null}
            {data?.degraded ? (
              <Notice tone="warning">
                Showing keyword matches only: searching by meaning isn't available right now (the
                model may still be loading). Exact words still work.
              </Notice>
            ) : null}
            {data?.suggestion ? (
              <p className="text-sm text-fg-muted">
                Did you mean{" "}
                <button
                  type="button"
                  className="font-medium text-accent-text italic underline decoration-accent/40 underline-offset-4 hover:decoration-accent"
                  onClick={() => go(updateParams(params, { q: data.suggestion ?? undefined }))}
                >
                  {data.suggestion}
                </button>
                ?
              </p>
            ) : null}

            {!data ? (
              query.isError ? null : (
                <ResultsSkeleton />
              )
            ) : data.results.length === 0 ? (
              page > 1 ? (
                <Notice
                  action={
                    <Button variant="secondary" size="sm" onClick={() => toPage(1)}>
                      First page
                    </Button>
                  }
                >
                  No more results past this point.
                </Notice>
              ) : (
                <NoResults
                  query={data.query}
                  filtered={activeFilters(params) > 0}
                  mode={data.mode}
                  onClearFilters={() => go(clearFilters(params))}
                  onHybrid={() => go(updateParams(params, { mode: undefined }))}
                />
              )
            ) : (
              <section aria-label="Results" className="grid gap-2">
                <header className="flex items-baseline justify-between gap-4 border-b border-border pb-2">
                  <h2 className="eyebrow">
                    {page > 1 ? `Page ${page} · ` : ""}
                    {data.results.length}
                    {data.has_more ? "+" : ""} {data.results.length === 1 ? "file" : "files"}
                  </h2>
                  <p className="hidden font-mono text-2xs text-fg-subtle sm:block">
                    {data.mode}
                    {data.reranked ? " · reranked" : ""} · {Math.round(data.timings.total_ms)} ms
                  </p>
                </header>
                <div
                  className={cn(
                    "transition-opacity",
                    query.isPlaceholderData && "opacity-55 motion-reduce:opacity-75",
                  )}
                >
                  {data.results.map((hit) => (
                    <SearchResult key={hit.file.id} hit={hit} />
                  ))}
                </div>
                {page > 1 || data.has_more ? (
                  <nav
                    aria-label="Result pages"
                    className="flex items-center justify-between border-t border-border pt-4"
                  >
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={page <= 1}
                      onClick={() => toPage(page - 1)}
                    >
                      <ChevronLeftIcon /> Previous
                    </Button>
                    <span className="font-mono text-xs text-fg-subtle">Page {page}</span>
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={!data.has_more}
                      onClick={() => toPage(page + 1)}
                    >
                      Next <ChevronRightIcon />
                    </Button>
                  </nav>
                ) : null}
                <SearchDebug data={data} />
                <p className="sr-only">
                  {RESULTS_PER_PAGE} results per page. Select a passage to open the file there.
                </p>
              </section>
            )}
          </>
        )}
      </div>
    </div>
  );
}
