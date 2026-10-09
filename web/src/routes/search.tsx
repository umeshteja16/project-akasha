import { getRouteApi, useNavigate } from "@tanstack/react-router";
import { SearchIcon } from "lucide-react";
import { useState } from "react";
import { EmptyState } from "@/components/common/empty-state";
import { PageHeader } from "@/components/common/page-header";
import { Input } from "@/components/ui/input";

const route = getRouteApi("/app/search");

const EXAMPLES = ["when does the lease renew", "notes on attention", "receipt from Lisbon"];

export function SearchPage() {
  const { q } = route.useSearch();
  const navigate = useNavigate();
  const [draft, setDraft] = useState(q ?? "");
  return (
    <div className="grid gap-8">
      <PageHeader
        eyebrow="Find"
        title="Search"
        description="Search by words or by meaning across every page you've kept. Results show the passage, not just the file."
      />
      {/* biome-ignore lint/a11y/useSemanticElements: <search> is unknown to jsdom (tests) */}
      <form
        role="search"
        className="relative"
        onSubmit={(event) => {
          event.preventDefault();
          const next = draft.trim();
          void navigate({ to: "/search", search: next ? { q: next } : {}, replace: true });
        }}
      >
        <SearchIcon
          className="pointer-events-none absolute top-1/2 left-3.5 size-4 -translate-y-1/2 text-fg-subtle"
          aria-hidden
        />
        <Input
          data-search-field
          type="search"
          value={draft}
          onChange={(e) => setDraft(e.currentTarget.value)}
          placeholder="Search your library"
          aria-label="Search your library"
          className="h-11 pl-10 text-base"
        />
      </form>
      {q ? (
        <p className="text-sm text-fg-muted" aria-live="polite">
          Results for “{q}” arrive with the search screen in the next update.
        </p>
      ) : null}
      <EmptyState icon={SearchIcon} title="Ask in your own words">
        <p>Exact phrases, half-remembered ideas, a name from a scanned receipt: all of it works.</p>
        <ul className="mt-5 flex flex-wrap justify-center gap-2" aria-label="Example searches">
          {EXAMPLES.map((example) => (
            <li
              key={example}
              className="rounded-full border border-border bg-surface px-3 py-1 font-mono text-xs text-fg-muted"
            >
              {example}
            </li>
          ))}
        </ul>
      </EmptyState>
    </div>
  );
}
