import { type Span, splitHighlights } from "@/lib/highlight";

/** Text with server highlight spans as <mark> (text nodes only, never HTML). */
export function Highlighted({ text, spans }: { text: string; spans: readonly Span[] }) {
  return (
    <>
      {splitHighlights(text, spans).map((run, i) =>
        run.mark ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: runs of one text never reorder
          <mark key={i} className="text-fg">
            {run.text}
          </mark>
        ) : (
          run.text
        ),
      )}
    </>
  );
}
