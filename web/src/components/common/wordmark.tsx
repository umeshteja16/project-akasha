import { cn } from "@/lib/utils";

/** "Akasha" set in the display serif, with the highlight dot as its only flourish. */
export function Wordmark({ className, size = "md" }: { className?: string; size?: "md" | "lg" }) {
  return (
    <span
      className={cn(
        "display inline-flex items-start gap-0.5 italic text-fg",
        size === "lg" ? "text-3xl" : "text-[1.5rem] leading-none",
        className,
      )}
    >
      Akasha
      <span
        aria-hidden
        className={cn(
          "rounded-full bg-highlight",
          size === "lg" ? "mt-1.5 size-2" : "mt-0.5 size-1.5",
        )}
      />
    </span>
  );
}
