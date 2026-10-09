import { SearchIcon } from "lucide-react";
import { EmptyState } from "@/components/common/empty-state";
import { PageHeader } from "@/components/common/page-header";

const EXAMPLES = ["when does the lease renew", "notes on attention", "receipt from Lisbon"];

export function SearchPage() {
  return (
    <div className="grid gap-8">
      <PageHeader
        eyebrow="Find"
        title="Search"
        description="Search by words or by meaning across every page you've kept. Results show the passage, not just the file."
      />
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
