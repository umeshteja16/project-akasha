import { type ComponentProps, type ReactNode, useId } from "react";
import { cn } from "@/lib/utils";
import { Input } from "./input";
import { Label } from "./label";

interface FieldProps extends Omit<ComponentProps<typeof Input>, "id"> {
  label: string;
  hint?: ReactNode;
  error?: string | null;
  /** Shown at the right of the label row (e.g. a "forgot?" link). */
  aside?: ReactNode;
}

/** Label + input + hint/error, wired up for screen readers. */
export function Field({ label, hint, error, aside, className, ...props }: FieldProps) {
  const id = useId();
  const describedBy = error ? `${id}-error` : hint ? `${id}-hint` : undefined;
  return (
    <div className={cn("grid gap-1.5", className)}>
      <div className="flex items-baseline justify-between gap-2">
        <Label htmlFor={id}>{label}</Label>
        {aside}
      </div>
      <Input
        id={id}
        aria-invalid={error ? true : undefined}
        aria-describedby={describedBy}
        {...props}
      />
      {error ? (
        <p id={`${id}-error`} className="text-xs text-danger">
          {error}
        </p>
      ) : hint ? (
        <p id={`${id}-hint`} className="text-xs text-fg-subtle">
          {hint}
        </p>
      ) : null}
    </div>
  );
}
