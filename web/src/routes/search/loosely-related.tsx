import { useQuery } from "@tanstack/react-query";
import { ChevronDownIcon } from "lucide-react";
import { useId, useState } from "react";
import { useApi } from "@/api/context";
import { searchQuery } from "@/api/search";
import { Button } from "@/components/ui/button";
import type { SearchParams } from "@/features/search/search-params";
import { cn } from "@/lib/utils";
import { SearchResult } from "./search-result";
import { ResultsSkeleton } from "./search-states";

/** The most loosely related files fetched when the section is opened. */
const LIMIT = 50;

/**
 * Files that are only close in meaning (below the relevance floor): collapsed
 * under the real results, fetched when opened.
 */
export function LooselyRelated({ params, count }: { params: SearchParams; count: number }) {
  const api = useApi();
  const [open, setOpen] = useState(false);
  const panel = useId();
  const query = useQuery({
    ...searchQuery(api, { ...params, page: undefined }, LIMIT, true),
    enabled: open && Boolean(params.q),
  });
  const weak = query.data?.results.filter((hit) => hit.loosely_related) ?? [];

  return (
    <section aria-label="Loosely related" className="grid gap-2 border-t border-border pt-4">
      <Button
        variant="ghost"
        size="sm"
        className="justify-self-start text-fg-muted"
        aria-expanded={open}
        aria-controls={panel}
        onClick={() => setOpen((o) => !o)}
      >
        <ChevronDownIcon
          className={cn("transition-transform motion-reduce:transition-none", open && "rotate-180")}
          aria-hidden
        />
        {open ? "Hide" : "Show"} {count} loosely related {count === 1 ? "file" : "files"}
      </Button>
      <div id={panel} hidden={!open} className="grid gap-2">
        <p className="max-w-[var(--reading-max)] text-sm text-fg-subtle">
          These are close in meaning to your search but don't contain its words, and the relevance
          check rated them low. They are probably not what you're looking for.
        </p>
        {open && query.isPending ? <ResultsSkeleton /> : null}
        {query.isError ? (
          <p className="text-sm text-danger">Couldn't load them. Try again in a moment.</p>
        ) : null}
        <div>
          {weak.map((hit) => (
            <SearchResult key={hit.file.id} hit={hit} />
          ))}
        </div>
      </div>
    </section>
  );
}
