// Split a window of extracted text at PDF page boundaries.
//
// The server counts offsets in Unicode characters (code points), not JavaScript
// UTF-16 units, so the text is handled as an array of code points.

import type { Schemas } from "@/api/client";

type PageSpan = Schemas["PageSpan"];

export interface Segment {
  /** 1-based page number; `null` for text outside any page (or unpaged formats). */
  page: number | null;
  source: PageSpan["source"] | null;
  text: string;
  /** Character (code point) offset of `text` in the whole extracted text. */
  start: number;
  /** Whether the page starts inside this window (show its marker). */
  startsHere: boolean;
}

export function segmentByPages(
  text: string,
  offset: number,
  pages: readonly PageSpan[],
): Segment[] {
  const chars = Array.from(text);
  const end = offset + chars.length;
  const slice = (from: number, to: number) =>
    chars.slice(Math.max(0, from - offset), Math.max(0, to - offset)).join("");
  const inWindow = pages.filter((p) => p.char_end > offset && p.char_start < end);
  if (inWindow.length === 0) {
    return chars.length
      ? [{ page: null, source: null, text, start: offset, startsHere: false }]
      : [];
  }
  const segments: Segment[] = [];
  let cursor = offset;
  for (const page of inWindow) {
    const from = Math.max(page.char_start, offset);
    const to = Math.min(page.char_end, end);
    if (from > cursor) {
      const gap = slice(cursor, from);
      if (gap.trim()) {
        segments.push({ page: null, source: null, text: gap, start: cursor, startsHere: false });
      }
    }
    segments.push({
      page: page.number,
      source: page.source,
      text: slice(from, to),
      start: from,
      startsHere: page.char_start >= offset,
    });
    cursor = Math.max(cursor, to);
  }
  if (cursor < end) {
    const tail = slice(cursor, end);
    if (tail.trim()) {
      segments.push({ page: null, source: null, text: tail, start: cursor, startsHere: false });
    }
  }
  return segments;
}

export interface Piece {
  text: string;
  /** Inside the passage being shown. */
  passage: boolean;
}

/**
 * Split a segment at the passage `[start, end)` (code points into the whole text),
 * so the passage can be marked and scrolled to.
 */
export function splitAtPassage(
  segment: Pick<Segment, "text" | "start">,
  passage: { start: number; end: number } | null,
): Piece[] {
  if (!passage) return [{ text: segment.text, passage: false }];
  const chars = Array.from(segment.text);
  const from = Math.max(0, passage.start - segment.start);
  const to = Math.min(chars.length, passage.end - segment.start);
  if (to <= 0 || from >= chars.length || to <= from) {
    return [{ text: segment.text, passage: false }];
  }
  const pieces: Piece[] = [];
  if (from > 0) pieces.push({ text: chars.slice(0, from).join(""), passage: false });
  pieces.push({ text: chars.slice(from, to).join(""), passage: true });
  if (to < chars.length) pieces.push({ text: chars.slice(to).join(""), passage: false });
  return pieces;
}
