// Files: types, query keys and query options shared by the library, the file page
// and the uploader. Mutations live in `features/files/mutations.ts`.

import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import { type Api, isApiError, type Schemas, unwrap } from "./client";

export type FileItem = Schemas["FileResponse"];
export type FileDetail = Schemas["FileDetail"];
export type FileStatus = Schemas["FileStatus"];
export type FileCategory = Schemas["FileCategory"];
export type FileSort = Schemas["FileSort"];
export type FileInfo = Schemas["FileInfo"];
export type Extraction = Schemas["ExtractionResponse"];
export type TagSummary = Schemas["TagSummary"];

/** Filters of the library list (from the URL); `sort` comes from the saved preference. */
export interface FileFilters {
  category?: FileCategory;
  tag?: string;
  pinned?: boolean;
}

export const fileKeys = {
  all: ["files"] as const,
  lists: () => [...fileKeys.all, "list"] as const,
  list: (filters: FileFilters, sort: FileSort) =>
    [...fileKeys.lists(), { ...filters, sort }] as const,
  detail: (id: string) => [...fileKeys.all, "detail", id] as const,
  extraction: (id: string) => [...fileKeys.all, "extraction", id] as const,
  similar: (id: string) => [...fileKeys.all, "similar", id] as const,
  tags: ["tags"] as const,
};

export const PAGE_SIZE = 48;

/** How often to poll while something is still being processed. */
export const POLL_MS = 2000;

export function isTerminal(status: FileStatus | string): boolean {
  return status === "ready" || status === "failed";
}

/**
 * `refetchInterval` for lists: poll only while a visible file is still pending or
 * processing, stop once everything is ready or failed.
 */
export function pollWhileProcessing(
  files: ReadonlyArray<{ status: string }> | undefined,
  ms: number = POLL_MS,
): number | false {
  return files?.some((f) => !isTerminal(f.status)) ? ms : false;
}

export const filesQuery = (
  api: Api,
  filters: FileFilters,
  sort: FileSort,
  pollMs: number = POLL_MS,
) =>
  infiniteQueryOptions({
    queryKey: fileKeys.list(filters, sort),
    queryFn: ({ pageParam, signal }) =>
      unwrap(
        api.GET("/api/v1/files", {
          params: {
            query: {
              sort,
              limit: PAGE_SIZE,
              ...(filters.category ? { category: filters.category } : {}),
              ...(filters.tag ? { tag: filters.tag } : {}),
              ...(filters.pinned ? { pinned: true } : {}),
              ...(pageParam ? { cursor: pageParam } : {}),
            },
          },
          signal,
        }),
      ),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor ?? null,
    refetchInterval: (query) =>
      pollWhileProcessing(
        query.state.data?.pages.flatMap((p) => p.items),
        pollMs,
      ),
  });

export const fileQuery = (api: Api, id: string, pollMs: number = POLL_MS) =>
  queryOptions({
    queryKey: fileKeys.detail(id),
    queryFn: ({ signal }) =>
      unwrap(api.GET("/api/v1/files/{id}", { params: { path: { id } }, signal })),
    refetchInterval: (query) => {
      const file = query.state.data;
      if (!file) return false;
      if (!isTerminal(file.status)) return pollMs;
      return awaitingEnrichment(file) ? pollMs * 2 : false;
    },
  });

/** A file that just became ready may still get a summary for a minute or so. */
export function awaitingEnrichment(file: FileItem, now: number = Date.now()): boolean {
  if (file.status !== "ready" || file.enrichment) return false;
  return now - Date.parse(file.updated_at) < 90_000;
}

export const extractionQuery = (api: Api, id: string, enabled: boolean) =>
  infiniteQueryOptions({
    queryKey: fileKeys.extraction(id),
    queryFn: async ({ pageParam, signal }) => {
      try {
        return await unwrap(
          api.GET("/api/v1/files/{id}/extraction", {
            params: { path: { id }, query: { offset: pageParam, limit: 50_000 } },
            signal,
          }),
        );
      } catch (error) {
        // Not extracted (yet): an empty state, not an error.
        if (isApiError(error) && error.status === 404) return null;
        throw error;
      }
    },
    initialPageParam: 0,
    getNextPageParam: (last) => last?.next_offset ?? undefined,
    enabled,
    staleTime: 5 * 60_000,
  });

export const similarQuery = (api: Api, id: string, enabled: boolean) =>
  queryOptions({
    queryKey: fileKeys.similar(id),
    queryFn: ({ signal }) =>
      unwrap(
        api.GET("/api/v1/files/{id}/similar", {
          params: { path: { id }, query: { limit: 6 } },
          signal,
        }),
      ),
    enabled,
  });

export const tagsQuery = (api: Api) =>
  queryOptions({
    queryKey: fileKeys.tags,
    queryFn: ({ signal }) => unwrap(api.GET("/api/v1/tags", { signal })),
    staleTime: 60_000,
  });

export function downloadUrl(id: string, inline = false): string {
  return `/api/v1/files/${encodeURIComponent(id)}/download${inline ? "?inline=true" : ""}`;
}

export function thumbnailUrl(id: string): string {
  return `/api/v1/files/${encodeURIComponent(id)}/thumbnail`;
}
