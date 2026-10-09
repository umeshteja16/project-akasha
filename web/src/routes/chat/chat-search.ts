import { parseFileScope } from "@/features/chat/scope";

export interface ChatSearch {
  /** Comma-separated file ids the answers are limited to. */
  files?: string;
}

export function validateChatSearch(search: Record<string, unknown>): ChatSearch {
  const ids = parseFileScope(search.files);
  return ids.length ? { files: ids.join(",") } : {};
}
