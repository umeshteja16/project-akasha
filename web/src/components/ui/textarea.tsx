import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export function Textarea({ className, ...props }: ComponentProps<"textarea">) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        "w-full min-w-0 resize-none rounded-md border border-border-strong/60 bg-surface px-3 py-2.5 text-sm text-fg shadow-xs",
        "placeholder:text-fg-subtle transition-[border-color,box-shadow]",
        "hover:border-border-strong focus-visible:border-accent focus-visible:outline-none",
        "focus-visible:ring-3 focus-visible:ring-accent/20 disabled:cursor-not-allowed disabled:opacity-60",
        className,
      )}
      {...props}
    />
  );
}
