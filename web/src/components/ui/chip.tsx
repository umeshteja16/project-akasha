import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

/** A pill toggle for filters; `pressed` marks it as on. */
export function Chip({
  pressed,
  className,
  ...props
}: ComponentProps<"button"> & { pressed?: boolean }) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      data-state={pressed ? "on" : "off"}
      className={cn(
        "inline-flex h-8 shrink-0 items-center gap-1.5 rounded-full border px-3 text-xs font-medium transition-colors [&_svg]:size-3.5",
        pressed
          ? "border-accent/40 bg-accent-soft text-accent-text"
          : "border-border bg-surface text-fg-muted hover:border-border-strong hover:text-fg",
        className,
      )}
      {...props}
    />
  );
}
