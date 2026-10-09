import { MessageSquareQuoteIcon, Trash2Icon, XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";

interface SelectionBarProps {
  count: number;
  total: number;
  onSelectAll: () => void;
  onClear: () => void;
  onDelete: () => void;
  /** Start a chat that answers only from the selected files. */
  onAsk: () => void;
}

/** Sticky bar with bulk actions while files are selected. */
export function SelectionBar({
  count,
  total,
  onSelectAll,
  onClear,
  onDelete,
  onAsk,
}: SelectionBarProps) {
  return (
    <section
      aria-label="Selection"
      className="sticky top-3 z-20 flex animate-fade-in items-center gap-2 rounded-lg border border-border bg-surface/95 py-1.5 pr-1.5 pl-2 shadow-md backdrop-blur-sm"
    >
      <Button variant="ghost" size="icon-sm" aria-label="Clear selection" onClick={onClear}>
        <XIcon />
      </Button>
      <p className="flex-1 text-sm text-fg" aria-live="polite">
        <span className="font-medium">{count}</span> selected
        {count < total ? (
          <Button variant="link" size="sm" className="ml-3 h-auto text-xs" onClick={onSelectAll}>
            Select all {total}
          </Button>
        ) : null}
      </p>
      <Button variant="secondary" size="sm" onClick={onAsk} disabled={count > 100}>
        <MessageSquareQuoteIcon />
        <span className="hidden sm:inline">Ask about {count === 1 ? "this" : "these"}</span>
        <span className="sm:hidden">Ask</span>
      </Button>
      <Button variant="danger" size="sm" onClick={onDelete}>
        <Trash2Icon />
        Delete
      </Button>
    </section>
  );
}
