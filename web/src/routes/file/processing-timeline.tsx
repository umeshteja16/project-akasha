import { CheckIcon, CircleDashedIcon, LoaderIcon, MinusIcon, XIcon } from "lucide-react";
import type { FileDetail } from "@/api/files";
import { formatDateTime, formatRelative } from "@/lib/format";
import { cn } from "@/lib/utils";
import { type StepState, timelineSteps } from "./timeline";

const ICON: Record<StepState, typeof CheckIcon> = {
  done: CheckIcon,
  active: LoaderIcon,
  failed: XIcon,
  waiting: CircleDashedIcon,
  skipped: MinusIcon,
};

const STATE_LABEL: Record<StepState, string> = {
  done: "done",
  active: "in progress",
  failed: "failed",
  waiting: "waiting",
  skipped: "skipped",
};

/** Uploaded → read → indexed → described, with live state. */
export function ProcessingTimeline({ file, chatModel }: { file: FileDetail; chatModel: boolean }) {
  const steps = timelineSteps(file, chatModel);
  return (
    <ol className="grid" aria-label="Processing">
      {steps.map((step, i) => {
        const Icon = ICON[step.state];
        const last = i === steps.length - 1;
        return (
          <li
            key={step.key}
            className="relative grid grid-cols-[1.5rem_minmax(0,1fr)] gap-3 pb-4 last:pb-0"
          >
            {last ? null : (
              <span
                aria-hidden
                className={cn(
                  "absolute top-6 bottom-0 left-[0.71875rem] w-px",
                  step.state === "done" ? "bg-accent/40" : "bg-border",
                )}
              />
            )}
            <span
              className={cn(
                "relative z-10 grid size-6 place-items-center rounded-full border",
                step.state === "done" && "border-accent/40 bg-accent-soft text-accent-text",
                step.state === "active" && "border-border-strong/50 bg-surface text-fg",
                step.state === "failed" && "border-danger/40 bg-danger-soft text-danger",
                (step.state === "waiting" || step.state === "skipped") &&
                  "border-border bg-surface text-fg-subtle",
              )}
            >
              <Icon
                className={cn(
                  "size-3",
                  step.state === "active" && "animate-spin motion-reduce:animate-none",
                )}
                aria-hidden
              />
            </span>
            <div className="min-w-0 pt-0.5">
              <p
                className={cn(
                  "text-sm",
                  step.state === "waiting" || step.state === "skipped"
                    ? "text-fg-muted"
                    : "text-fg",
                )}
              >
                {step.label}
                <span className="sr-only">: {STATE_LABEL[step.state]}</span>
              </p>
              {step.detail ? (
                <p
                  className={cn(
                    "text-xs",
                    step.state === "failed" ? "text-danger" : "text-fg-subtle",
                  )}
                >
                  {step.detail}
                </p>
              ) : null}
              {step.at ? (
                <time
                  dateTime={step.at}
                  title={formatDateTime(step.at)}
                  className="text-xs text-fg-subtle"
                >
                  {formatRelative(step.at)}
                </time>
              ) : null}
            </div>
          </li>
        );
      })}
    </ol>
  );
}
