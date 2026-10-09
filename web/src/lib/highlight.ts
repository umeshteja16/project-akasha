// Highlight spans from the server are `[start, end)` in Unicode characters (code
// points), but JavaScript strings index UTF-16 units: an emoji or a rare CJK
// character counts once on the server and twice here. Convert, then split the text
// into plain and marked runs (rendered as text nodes and <mark>, never as HTML).

export interface Span {
  start: number;
  end: number;
}

export interface Run {
  text: string;
  mark: boolean;
}

/** UTF-16 index of every code-point offset 0..=n (`n` = the text's length in code points). */
export function codePointIndex(text: string): number[] {
  const index: number[] = [];
  let unit = 0;
  for (const ch of text) {
    index.push(unit);
    unit += ch.length;
  }
  index.push(unit);
  return index;
}

/**
 * Split `text` into runs at the highlight spans. Spans are clamped to the text,
 * sorted, and overlapping or touching spans are merged; empty ones are dropped.
 */
export function splitHighlights(text: string, spans: readonly Span[]): Run[] {
  const index = codePointIndex(text);
  const max = index.length - 1;
  const sorted = spans
    .map((s) => ({
      start: Math.max(0, Math.min(max, Math.trunc(s.start))),
      end: Math.max(0, Math.min(max, Math.trunc(s.end))),
    }))
    .filter((s) => s.end > s.start)
    .sort((a, b) => a.start - b.start);
  const merged: Span[] = [];
  for (const span of sorted) {
    const last = merged[merged.length - 1];
    if (last && span.start <= last.end) last.end = Math.max(last.end, span.end);
    else merged.push({ ...span });
  }
  const runs: Run[] = [];
  let cursor = 0;
  for (const span of merged) {
    if (span.start > cursor) {
      runs.push({ text: text.slice(index[cursor], index[span.start]), mark: false });
    }
    runs.push({ text: text.slice(index[span.start], index[span.end]), mark: true });
    cursor = span.end;
  }
  if (cursor < max) runs.push({ text: text.slice(index[cursor]), mark: false });
  return runs;
}
