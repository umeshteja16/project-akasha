import { Link } from "@tanstack/react-router";
import {
  CheckIcon,
  CircleAlertIcon,
  CopyIcon,
  RotateCcwIcon,
  SearchXIcon,
  SquareIcon,
  TriangleAlertIcon,
} from "lucide-react";
import { Fragment, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import type { AnswerStatus, Citation } from "@/api/chat";
import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import { Tooltip } from "@/components/ui/tooltip";
import { Markdown } from "@/lib/markdown";
import { cn } from "@/lib/utils";
import { CitationChip } from "./citation-chip";
import { splitCitations } from "./citations";
import { SourcesPanel } from "./sources-panel";

export type AnswerState = "sending" | "streaming" | AnswerStatus;

export interface AnswerView {
  text: string;
  state: AnswerState;
  /** Passages `[n]` may refer to (chips). */
  citable: Citation[];
  /** Passages listed under the answer. */
  cited: Citation[];
  model?: string | null;
  latencyMs?: number | null;
  error?: string;
}

/** Answer text as safe Markdown, `[n]` as citation chips. */
export function AnswerText({
  text,
  citations,
  streaming,
}: {
  text: string;
  citations: Citation[];
  streaming?: boolean;
}) {
  const byNumber = useMemo(() => new Map(citations.map((c) => [c.n, c])), [citations]);
  return (
    <div className="relative">
      <Markdown
        source={text}
        className="gap-3 font-display text-[1.0625rem] leading-[1.7]"
        renderText={(t, key) => (
          <Fragment key={key}>
            {splitCitations(t, (n) => byNumber.has(n)).map((piece, i) => {
              if (typeof piece === "string") return piece;
              const c = byNumber.get(piece.n);
              // biome-ignore lint/suspicious/noArrayIndexKey: pieces of one text never reorder
              return c ? <CitationChip key={i} citation={c} /> : null;
            })}
          </Fragment>
        )}
      />
      {streaming ? (
        <span
          aria-hidden
          className="ml-0.5 inline-block h-[1.1em] w-[2px] translate-y-[3px] animate-caret bg-accent align-baseline motion-reduce:animate-none"
        />
      ) : null}
    </div>
  );
}

export function copyText(view: AnswerView): string {
  if (view.cited.length === 0) return view.text;
  const lines = view.cited.map((c) => `[${c.n}] ${c.file_name}${c.page ? `, p. ${c.page}` : ""}`);
  return `${view.text}\n\nSources:\n${lines.join("\n")}`;
}

/** Longest answer text read out when it is ready (the rest is on screen). */
const ANNOUNCE_CHARS = 600;

/**
 * What a screen reader hears about a live answer: its progress and, once done,
 * the answer itself. Not every streamed piece: that would be unlistenable.
 * Stored answers (never seen live) are not announced.
 */
export function announcement(view: AnswerView, wasLive: boolean): string {
  switch (view.state) {
    case "sending":
      return "Searching your files.";
    case "streaming":
      return "Writing the answer.";
    case "answered":
    case "no_llm": {
      if (!wasLive) return "";
      const text = view.text
        .replace(/\s*\[\d+\]/g, "")
        .replace(/\s+/g, " ")
        .trim();
      const cut = text.length > ANNOUNCE_CHARS ? `${text.slice(0, ANNOUNCE_CHARS)}…` : text;
      return `Answer ready. ${cut}`;
    }
    case "refused":
      return wasLive ? "No answer: nothing in your files was close enough." : "";
    case "cancelled":
      return wasLive ? "Stopped." : "";
    default:
      return "";
  }
}

function useAnnouncement(view: AnswerView): string {
  const live = view.state === "sending" || view.state === "streaming";
  const wasLive = useRef(live);
  useEffect(() => {
    if (live) wasLive.current = true;
  }, [live]);
  return announcement(view, wasLive.current || live);
}

/** One answer: its text, state notes, sources and actions. */
export function Answer({
  view,
  question,
  onRetry,
  onStop,
}: {
  view: AnswerView;
  question: string;
  /** Ask the same question again (regenerate / try again). */
  onRetry?: () => void;
  onStop?: () => void;
}) {
  const { state } = view;
  const live = state === "sending" || state === "streaming";
  const spoken = useAnnouncement(view);

  let body: ReactNode;
  if (state === "sending" || (state === "streaming" && !view.text)) {
    body = (
      <p className="flex items-center gap-2.5 text-sm text-fg-muted">
        <Dots />
        {state === "sending"
          ? "Searching your files…"
          : `Reading ${view.citable.length} ${view.citable.length === 1 ? "passage" : "passages"}…`}
      </p>
    );
  } else if (state === "refused") {
    body = (
      <div className="grid gap-2 rounded-lg border border-border bg-surface-2/50 px-4 py-3">
        <p className="flex items-center gap-2 font-display text-[1.0625rem] text-fg">
          <SearchXIcon className="size-4 shrink-0 text-fg-subtle" aria-hidden />
          {view.text || "I couldn't find this in your files."}
        </p>
        <p className="text-xs text-fg-muted">
          Akasha only answers from your files, and nothing it found was close enough. Try other
          words, check the file has finished reading, or{" "}
          <Link to="/search" search={{ q: question }} className="text-accent-text hover:underline">
            search for related passages
          </Link>
          .
        </p>
      </div>
    );
  } else if (state === "no_llm") {
    body = (
      <div className="grid gap-2 rounded-lg border border-border bg-surface-2/50 px-4 py-3">
        <p className="flex items-start gap-2 text-sm text-fg">
          <TriangleAlertIcon className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden />
          <span>
            No language model is configured, so here are the most relevant passages instead of a
            written answer.
          </span>
        </p>
        <p className="text-xs text-fg-muted">
          To get answers, set <code className="font-mono">AKASHA_LLM_PROVIDER</code> (Ollama,
          Claude, Gemini or any OpenAI-compatible server) and restart Akasha.
        </p>
      </div>
    );
  } else {
    body = view.text ? (
      <AnswerText text={view.text} citations={view.citable} streaming={state === "streaming"} />
    ) : null;
  }

  return (
    <div className="grid gap-3">
      <p className="sr-only" aria-live="polite" aria-atomic="true">
        {spoken}
      </p>
      <div className="grid gap-3" aria-busy={live}>
        {body}
      </div>
      {state === "error" ? (
        <p
          role="alert"
          className="flex items-start gap-2 rounded-lg border border-danger/30 bg-danger-soft px-3 py-2 text-sm text-fg"
        >
          <CircleAlertIcon className="mt-0.5 size-4 shrink-0 text-danger" aria-hidden />
          <span className="flex-1">{view.error ?? "The answer failed."}</span>
          {onRetry ? (
            <Button variant="secondary" size="sm" onClick={onRetry}>
              <RotateCcwIcon /> Try again
            </Button>
          ) : null}
        </p>
      ) : null}
      {state === "cancelled" ? (
        <p className="inline-flex items-center gap-1.5 text-xs text-fg-subtle">
          <SquareIcon className="size-3" aria-hidden /> Stopped
          {view.text ? "" : " before an answer was written"}
        </p>
      ) : null}
      {state === "answered" || state === "no_llm" ? (
        <SourcesPanel citations={view.cited} defaultOpen={state === "no_llm"} />
      ) : null}
      <Actions
        view={view}
        onRetry={state === "error" ? undefined : onRetry}
        onStop={live ? onStop : undefined}
      />
    </div>
  );
}

function Actions({
  view,
  onRetry,
  onStop,
}: {
  view: AnswerView;
  onRetry?: () => void;
  onStop?: () => void;
}) {
  const [copied, setCopied] = useState(false);
  if (onStop) return null;
  const canCopy = Boolean(view.text) && view.state !== "refused";
  const meta = [
    view.model,
    view.latencyMs ? `${(view.latencyMs / 1000).toFixed(1)} s` : null,
  ].filter(Boolean);
  return (
    <div className="flex items-center gap-1 text-fg-subtle">
      {canCopy ? (
        <Tooltip content={copied ? "Copied" : "Copy answer"}>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Copy answer"
            onClick={() => {
              void navigator.clipboard
                ?.writeText(copyText(view))
                .then(() => {
                  setCopied(true);
                  window.setTimeout(() => setCopied(false), 1500);
                })
                .catch(() => toast({ title: "Couldn't copy", tone: "danger" }));
            }}
          >
            {copied ? <CheckIcon /> : <CopyIcon />}
          </Button>
        </Tooltip>
      ) : null}
      {onRetry ? (
        <Tooltip content="Ask again">
          <Button variant="ghost" size="icon-sm" aria-label="Ask again" onClick={onRetry}>
            <RotateCcwIcon />
          </Button>
        </Tooltip>
      ) : null}
      {meta.length ? (
        <span className={cn("ml-2 font-mono text-2xs", !canCopy && !onRetry && "ml-0")}>
          {meta.join(" · ")}
        </span>
      ) : null}
    </div>
  );
}

function Dots() {
  return (
    <span className="inline-flex gap-1" aria-hidden>
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="size-1.5 animate-pulse-soft rounded-full bg-accent motion-reduce:animate-none"
          style={{ animationDelay: `${i * 160}ms` }}
        />
      ))}
    </span>
  );
}
