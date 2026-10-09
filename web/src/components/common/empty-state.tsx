import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

interface EmptyStateProps {
  icon: LucideIcon;
  title: string;
  children?: ReactNode;
  actions?: ReactNode;
  className?: string;
}

/** A quiet, editorial empty state: ornament, serif title, one paragraph, actions. */
export function EmptyState({ icon: Icon, title, children, actions, className }: EmptyStateProps) {
  return (
    <div
      className={cn(
        "mx-auto flex max-w-lg flex-col items-center px-2 py-14 text-center sm:py-20",
        className,
      )}
    >
      <div className="flex items-center gap-3 text-border-strong" aria-hidden>
        <span className="h-px w-10 bg-border" />
        <span className="grid size-11 place-items-center rounded-full border border-border bg-surface text-accent-text shadow-xs">
          <Icon className="size-5" strokeWidth={1.6} />
        </span>
        <span className="h-px w-10 bg-border" />
      </div>
      <h2 className="display mt-6 text-2xl text-fg">{title}</h2>
      {children ? <div className="mt-2 text-sm text-fg-muted">{children}</div> : null}
      {actions ? <div className="mt-6 flex flex-wrap justify-center gap-2">{actions}</div> : null}
    </div>
  );
}
