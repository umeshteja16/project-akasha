import { ArrowUpIcon, SquareIcon } from "lucide-react";
import { type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export const MAX_QUESTION = 4000;

interface ComposerProps {
  onSend: (text: string) => void;
  onStop?: () => void;
  /** An answer is streaming: show stop instead of send. */
  streaming?: boolean;
  disabled?: boolean;
  autoFocus?: boolean;
  placeholder?: string;
  /** Shown above the text box (e.g. the file scope). */
  top?: ReactNode;
  /** Put this text in the box and focus it (edit an earlier question); a new `key` re-applies it. */
  draft?: { text: string; key: number };
}

/** Multi-line question box: Enter sends, Shift+Enter adds a line, stop while streaming. */
export function Composer({
  onSend,
  onStop,
  streaming,
  disabled,
  autoFocus,
  placeholder = "Ask about your files…",
  top,
  draft,
}: ComposerProps) {
  const [text, setText] = useState("");
  const ref = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (!draft) return;
    setText(draft.text);
    const el = ref.current;
    if (el) {
      el.focus();
      el.setSelectionRange(draft.text.length, draft.text.length);
    }
  }, [draft]);

  // Grow with the text up to ~8 lines.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-measure when the text changes
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  }, [text]);

  const send = () => {
    const question = text.trim();
    if (!question || streaming || disabled) return;
    onSend(question.slice(0, MAX_QUESTION));
    setText("");
  };

  return (
    <form
      className="grid gap-2 rounded-xl border border-border-strong/50 bg-surface p-2 shadow-md transition-[border-color,box-shadow] focus-within:border-accent focus-within:ring-3 focus-within:ring-accent/15"
      onSubmit={(e) => {
        e.preventDefault();
        send();
      }}
    >
      {top}
      <div className="flex items-end gap-2">
        <textarea
          ref={ref}
          rows={1}
          value={text}
          // biome-ignore lint/a11y/noAutofocus: the composer is the page's purpose
          autoFocus={autoFocus}
          maxLength={MAX_QUESTION}
          aria-label="Your question"
          placeholder={placeholder}
          onChange={(e) => setText(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              send();
            }
          }}
          className="max-h-[220px] min-h-10 flex-1 resize-none bg-transparent px-2 py-2 text-base text-fg outline-none placeholder:text-fg-subtle"
        />
        {streaming ? (
          <Button
            variant="secondary"
            size="icon"
            className="rounded-lg"
            aria-label="Stop answering"
            onClick={onStop}
          >
            <SquareIcon className="size-3.5 fill-current" />
          </Button>
        ) : (
          <Button
            type="submit"
            size="icon"
            className={cn("rounded-lg", !text.trim() && "opacity-40")}
            aria-label="Send question"
            disabled={disabled || !text.trim()}
          >
            <ArrowUpIcon />
          </Button>
        )}
      </div>
      <p className="hidden px-2 text-2xs text-fg-subtle sm:block">
        <kbd className="font-mono">Enter</kbd> to send · <kbd className="font-mono">Shift</kbd>+
        <kbd className="font-mono">Enter</kbd> for a new line · answers cite your files
      </p>
    </form>
  );
}
