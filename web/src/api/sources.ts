// Watched folders (`/api/v1/sources`): server folders imported and kept in sync.

import { queryOptions } from "@tanstack/react-query";
import { type Api, type Schemas, unwrap } from "./client";

export type Source = Schemas["SourceResponse"];
export type SourceList = Schemas["SourceList"];
export type SourceOnDelete = Schemas["SourceOnDelete"];

export const sourceKeys = { all: ["sources"] as const };

/** A scan is queued or running: worth polling. */
export function isSyncing(source: Source): boolean {
  return source.enabled && (source.status === "pending" || source.status === "scanning");
}

export const sourcesQuery = (api: Api) =>
  queryOptions({
    queryKey: sourceKeys.all,
    queryFn: ({ signal }) => unwrap(api.GET("/api/v1/sources", { signal })),
    // Poll while a folder is being scanned, so counts and status move on their own.
    refetchInterval: (query) => (query.state.data?.items.some(isSyncing) ? 2000 : false),
  });

/** Globs from a textarea: one per line, blanks dropped. */
export function parseGlobs(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

export type SourceState = "syncing" | "ok" | "error" | "paused";

export function sourceState(source: Source): SourceState {
  if (!source.enabled) return "paused";
  if (source.status === "error") return "error";
  if (isSyncing(source)) return "syncing";
  return "ok";
}
