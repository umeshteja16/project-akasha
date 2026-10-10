// Search state lives in the URL (`/search?q=..&mode=..&type=..`), so a search can be
// shared, reloaded, and walked with back/forward. Defaults are left out of the URL.

import type { FileCategory } from "@/api/files";
import type { SearchMode } from "@/api/search";

export interface SearchParams {
  q?: string;
  /** `hybrid` is the default and never appears in the URL. */
  mode?: Exclude<SearchMode, "hybrid">;
  type?: FileCategory;
  /** Uploaded on or after this day (`YYYY-MM-DD`). */
  from?: string;
  /** Uploaded on or before this day (inclusive). */
  to?: string;
  /** Comma-separated, lowercase tags; results carry all of them. */
  tags?: string;
  pinned?: true;
  /** Only files in this collection (its id). */
  collection?: string;
  /** 1-based; `1` is left out. */
  page?: number;
}

export const MAX_QUERY = 500;
const CATEGORIES: readonly string[] = ["pdf", "image", "text", "audio", "video"];
const DAY = /^\d{4}-\d{2}-\d{2}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function isDay(value: unknown): value is string {
  return typeof value === "string" && DAY.test(value) && !Number.isNaN(Date.parse(value));
}

/** Normalise a tag list: trimmed, lowercase, no empties or repeats. */
export function normalizeTags(raw: string | readonly string[]): string[] {
  const list = typeof raw === "string" ? raw.split(",") : raw;
  const out: string[] = [];
  for (const tag of list) {
    const t = tag.trim().toLowerCase();
    if (t && !out.includes(t)) out.push(t);
  }
  return out;
}

/** `validateSearch` for the search route: keep what is valid, drop the rest. */
export function validateSearchParams(search: Record<string, unknown>): SearchParams {
  const out: SearchParams = {};
  if (typeof search.q === "string" && search.q.trim()) out.q = search.q.trim().slice(0, MAX_QUERY);
  if (search.mode === "keyword" || search.mode === "semantic") out.mode = search.mode;
  if (typeof search.type === "string" && CATEGORIES.includes(search.type)) {
    out.type = search.type as FileCategory;
  }
  if (isDay(search.from)) out.from = search.from;
  if (isDay(search.to)) out.to = search.to;
  if (out.from && out.to && out.from > out.to) [out.from, out.to] = [out.to, out.from];
  const tags =
    typeof search.tags === "string"
      ? normalizeTags(search.tags)
      : Array.isArray(search.tags)
        ? normalizeTags(search.tags.filter((t): t is string => typeof t === "string"))
        : [];
  if (tags.length) out.tags = tags.join(",");
  if (search.pinned === true || search.pinned === "true") out.pinned = true;
  if (typeof search.collection === "string" && UUID.test(search.collection)) {
    out.collection = search.collection.toLowerCase();
  }
  const page = typeof search.page === "string" ? Number(search.page) : search.page;
  if (typeof page === "number" && Number.isInteger(page) && page > 1 && page <= 20) {
    out.page = page;
  }
  return out;
}

/** Merge changes into the params, dropping unset keys (and the page, unless given). */
export function updateParams(current: SearchParams, changes: Partial<SearchParams>): SearchParams {
  const next: SearchParams = { ...current, page: undefined, ...changes };
  for (const key of Object.keys(next) as (keyof SearchParams)[]) {
    if (next[key] === undefined || next[key] === "") delete next[key];
  }
  if (next.page === 1) delete next.page;
  return next;
}

/** The filters in effect (everything but the query, mode and page). */
export function activeFilters(p: SearchParams): number {
  return [p.type, p.from || p.to, p.tags, p.pinned, p.collection].filter(Boolean).length;
}

export function clearFilters(p: SearchParams): SearchParams {
  return updateParams(p, {
    type: undefined,
    from: undefined,
    to: undefined,
    tags: undefined,
    pinned: undefined,
    collection: undefined,
  });
}

/** Query string for `GET /api/v1/search`. */
export function toApiQuery(p: SearchParams & { q: string }, limit: number) {
  return {
    q: p.q,
    limit,
    ...(p.mode ? { mode: p.mode } : {}),
    ...(p.type ? { type: p.type } : {}),
    ...(p.from ? { from: p.from } : {}),
    ...(p.to ? { to: p.to } : {}),
    ...(p.tags ? { tags: p.tags } : {}),
    ...(p.pinned ? { pinned: true } : {}),
    ...(p.collection ? { collection_id: p.collection } : {}),
    ...(p.page ? { page: p.page } : {}),
  };
}

/** `YYYY-MM-DD` of the day `days` before `now` (local time). */
export function daysAgo(days: number, now: Date = new Date()): string {
  const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - days);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
