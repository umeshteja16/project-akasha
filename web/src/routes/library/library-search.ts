// Library filters live in the URL so a filtered view can be linked and survives reloads.

import type { FileCategory, FileFilters } from "@/api/files";

export interface LibrarySearch {
  type?: FileCategory;
  tag?: string;
  pinned?: true;
}

const CATEGORIES: readonly FileCategory[] = ["pdf", "image", "text", "audio", "video"];

export function validateLibrarySearch(search: Record<string, unknown>): LibrarySearch {
  const out: LibrarySearch = {};
  const type = search.type;
  if (typeof type === "string" && (CATEGORIES as readonly string[]).includes(type)) {
    out.type = type as FileCategory;
  }
  if (typeof search.tag === "string" && search.tag.trim())
    out.tag = search.tag.trim().toLowerCase();
  if (search.pinned === true || search.pinned === "true") out.pinned = true;
  return out;
}

export function toFilters(search: LibrarySearch): FileFilters {
  return {
    ...(search.type ? { category: search.type } : {}),
    ...(search.tag ? { tag: search.tag } : {}),
    ...(search.pinned ? { pinned: true } : {}),
  };
}

export function hasFilters(search: LibrarySearch): boolean {
  return Boolean(search.type || search.tag || search.pinned);
}
