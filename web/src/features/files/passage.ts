// Links into a file at a passage: `/files/<id>?at=<start>-<end>&page=<n>`. Search
// results and chat citations use them; the file page opens its text at the passage
// (marked and scrolled into view) and the PDF preview at the page.

export interface FileSearch {
  /** `start-end`: the passage in the extracted text, in characters (code points). */
  at?: string;
  /** 1-based PDF page. */
  page?: number;
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
  return out;
}

export function parsePassage(at: string | undefined): Passage | null {
  const match = /^(\d{1,9})-(\d{1,9})$/.exec(at ?? "");
  if (!match) return null;
  const start = Number(match[1]);
  const end = Number(match[2]);
  return end > start ? { start, end } : null;
}

/** Search params of a link to a passage (page optional). */
export function passageSearch(
  start: number,
  end: number,
  page?: number | null,
): Required<Pick<FileSearch, "at">> & FileSearch {
  return { at: `${start}-${end}`, ...(page ? { page } : {}) };
}
