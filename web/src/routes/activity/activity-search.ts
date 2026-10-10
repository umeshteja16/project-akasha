import type { ActivityCategory } from "@/api/activity";

export interface ActivitySearch {
  /** Show only this category (`security` is the audit log). */
  category?: ActivityCategory;
}

const CATEGORIES: readonly ActivityCategory[] = [
  "files",
  "search",
  "chat",
  "collections",
  "security",
];

export function validateActivitySearch(search: Record<string, unknown>): ActivitySearch {
  const category = CATEGORIES.find((c) => c === search.category);
  return category ? { category } : {};
}
