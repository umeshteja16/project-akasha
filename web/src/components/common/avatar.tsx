import { cn, initials } from "@/lib/utils";

export function Avatar({ name, className }: { name: string; className?: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "grid size-8 shrink-0 place-items-center rounded-full bg-accent-soft font-medium text-accent-text text-xs",
        className,
      )}
    >
      {initials(name)}
    </span>
  );
}
