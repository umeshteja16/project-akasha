import type { Extraction } from "@/api/files";

export interface TranscriptLine {
  startMs: number;
  endMs: number;
  /** Character span in the whole transcript (code points). */
  charStart: number;
  charEnd: number;
  text: string;
}

/**
 * The transcript's lines with their times, from the loaded text windows (which
 * start at offset 0 and follow each other). Lines past the loaded text are left out.
 */
export function transcriptLines(
  windows: Pick<Extraction, "offset" | "text">[],
  segments: Extraction["segments"],
): TranscriptLine[] {
  const chars: string[] = [];
  for (const win of windows) {
    if (win.offset !== chars.length) break;
    for (const c of win.text) chars.push(c);
  }
  const lines: TranscriptLine[] = [];
  for (const s of segments) {
    if (s.char_end > chars.length) break;
    lines.push({
      startMs: s.start_ms,
      endMs: s.end_ms,
      charStart: s.char_start,
      charEnd: s.char_end,
      text: chars.slice(s.char_start, s.char_end).join(""),
    });
  }
  return lines;
}

/** The line being spoken at `ms` (the last one started by then), or -1. */
export function lineAt(lines: TranscriptLine[], ms: number): number {
  let lo = 0;
  let hi = lines.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const line = lines[mid];
    if (line && line.startMs <= ms) {
      found = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return found;
}

/** Lines overlapping a passage (character span), for highlighting a search hit. */
export function linesInPassage(
  lines: TranscriptLine[],
  passage: { start: number; end: number } | null,
): Set<number> {
  const out = new Set<number>();
  if (!passage) return out;
  lines.forEach((line, i) => {
    if (line.charEnd > passage.start && line.charStart < passage.end) out.add(i);
  });
  return out;
}
