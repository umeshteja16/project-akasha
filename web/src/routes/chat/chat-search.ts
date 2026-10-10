import { parseFileScope } from "@/features/chat/scope";

export interface ChatSearch {
  /** Comma-separated file ids the answers are limited to. */
  files?: string;
  /** A collection the answers are limited to (from a collection page). */
  collection?: string;
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function validateChatSearch(search: Record<string, unknown>): ChatSearch {
  const ids = parseFileScope(search.files);
  const out: ChatSearch = ids.length ? { files: ids.join(",") } : {};
  if (typeof search.collection === "string" && UUID.test(search.collection)) {
    out.collection = search.collection.toLowerCase();
  }
  return out;
}
