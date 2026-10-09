import { cn } from "@/lib/utils";

interface ProgressProps {
  /** 0..1 */
  value: number;
  /** Accessible name. */
  label: string;
  className?: string;
}

/** A thin determinate progress bar. */
export function Progress({ value, label, className }: ProgressProps) {
  const pct = Math.round(Math.min(1, Math.max(0, value)) * 100);
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={pct}
      className={cn("h-1 w-full overflow-hidden rounded-full bg-surface-3", className)}
    >
      <div
        className="h-full rounded-full bg-accent transition-[width] duration-200 ease-out"
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}
