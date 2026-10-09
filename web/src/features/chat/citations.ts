// Inline citations: answers cite sources as `[n]`, sometimes `[1, 2]` or `[1][2]`.
// Only numbers that belong to a known source become chips; anything else (an
// array index in code, "[42]" in a quote) stays text.

export type Piece = string | { n: number };

const CITATION = /\[(\d{1,3}(?:\s*,\s*\d{1,3})*)\]/g;

export function splitCitations(text: string, known: (n: number) => boolean): Piece[] {
  const out: Piece[] = [];
  let last = 0;
  for (const match of text.matchAll(CITATION)) {
    const numbers = (match[1] ?? "").split(",").map((s) => Number(s.trim()));
    if (!numbers.every(known)) continue;
    const at = match.index ?? 0;
    if (at > last) out.push(text.slice(last, at));
    for (const n of numbers) out.push({ n });
    last = at + match[0].length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

/** The text without citation markers (for copying an answer as plain prose). */
export function stripCitations(text: string): string {
  return text.replace(/\s?\[\d{1,3}(?:\s*,\s*\d{1,3})*\]/g, "");
}
