// Links into a file at a passage: `/files/<id>?at=<start>-<end>&page=<n>&t=<s>`.
// Search results and chat citations use them; the file page opens its text at the
// passage (marked and scrolled into view), the PDF preview at the page, and a
// recording's player at second `t` (with that transcript line highlighted).

export interface FileSearch {
  /** `start-end`: the passage in the extracted text, in characters (code points). */
  at?: string;
  /** 1-based PDF page. */
  page?: number;
  /** Audio and video: start playing at this second. */
  t?: number;
}

export interface Passage {
  start: number;
  end: number;
}

export function validateFileSearch(search: Record<string, unknown>): FileSearch {
  const out: FileSearch = {};
  if (typeof search.at === "string" && parsePassage(search.at)) out.at = search.at;
  const page = typeof search.page === "string" ? Number(search.page) : search.page;
  if (typeof page === "number" && Number.isInteger(page) && page > 0) out.page = page;
  const t = typeof search.t === "string" ? Number(search.t) : search.t;
  if (typeof t === "number" && Number.isInteger(t) && t >= 0 && t < 1e7) out.t = t;
  return out;
}

export function parsePassage(at: string | undefined): Passage | null {
  const match = /^(\d{1,9})-(\d{1,9})$/.exec(at ?? "");
  if (!match) return null;
  const start = Number(match[1]);
  const end = Number(match[2]);
  return end > start ? { start, end } : null;
}

/** Search params of a link to a passage (page and recording time optional). */
export function passageSearch(
  start: number,
  end: number,
  page?: number | null,
  startMs?: number | null,
): Required<Pick<FileSearch, "at">> & FileSearch {
  return {
    at: `${start}-${end}`,
    ...(page ? { page } : {}),
    ...(startMs != null && startMs >= 0 ? { t: Math.floor(startMs / 1000) } : {}),
  };
}

/** A time in a recording as a clock: `4:05`, or `1:02:03` past an hour. */
export function formatTimestamp(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor(total / 60) % 60;
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

/** Where a passage is, for labels: "p. 3" in a PDF, "1:05" in a recording. */
export function locationLabel(at: {
  page?: number | null;
  start_ms?: number | null;
}): string | null {
  if (at.page) return `p. ${at.page}`;
  if (at.start_ms != null) return formatTimestamp(at.start_ms);
  return null;
}
