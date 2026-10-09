import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export function Input({ className, type = "text", ...props }: ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(
        "h-10 w-full min-w-0 rounded-md border border-border-strong/60 bg-surface px-3 text-sm text-fg shadow-xs",
        "placeholder:text-fg-subtle transition-[border-color,box-shadow]",
        "hover:border-border-strong focus-visible:border-accent focus-visible:outline-none",
        "focus-visible:ring-3 focus-visible:ring-accent/20",
        "aria-invalid:border-danger aria-invalid:ring-danger/15 disabled:cursor-not-allowed disabled:opacity-60",
        className,
      )}
      {...props}
    />
  );
}
