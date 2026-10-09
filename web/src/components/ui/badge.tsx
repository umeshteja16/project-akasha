import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center gap-1 rounded-sm px-1.5 py-0.5 text-2xs font-medium uppercase tracking-wider font-mono",
  {
    variants: {
      tone: {
        neutral: "bg-surface-2 text-fg-muted",
        accent: "bg-accent-soft text-accent-text",
        danger: "bg-danger-soft text-danger",
        success: "bg-surface-2 text-success",
        warning: "bg-surface-2 text-warning",
      },
    },
    defaultVariants: { tone: "neutral" },
  },
);

export function Badge({
  className,
  tone,
  ...props
}: ComponentProps<"span"> & VariantProps<typeof badgeVariants>) {
  return <span className={cn(badgeVariants({ tone }), className)} {...props} />;
}
