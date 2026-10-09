// Search: types and query options (`GET /api/v1/search`, file-grouped results).

import { keepPreviousData, queryOptions } from "@tanstack/react-query";
import { type SearchParams, toApiQuery } from "@/features/search/search-params";
import { type Api, isApiError, type Schemas, unwrap } from "./client";

export type SearchMode = Schemas["SearchMode"];
export type FileResults = Schemas["FileResults"];
export type FileHit = Schemas["FileHit"];
export type ChunkMatch = Schemas["ChunkMatch"];
export type Timings = Schemas["Timings"];

export const searchKeys = {
  all: ["search"] as const,
  results: (params: SearchParams, limit: number, includeWeak = false) =>
    [...searchKeys.all, params, limit, includeWeak] as const,
};

export const RESULTS_PER_PAGE = 10;

/** Don't retry client errors (bad query, rate limit): only flaky network/5xx, once. */
export function retrySearch(failures: number, error: unknown): boolean {
  if (isApiError(error) && error.status >= 400 && error.status < 500) return false;
  return failures < 1;
}

/**
 * File-grouped results for the URL's search state. Changing the query cancels the
 * request in flight (the signal is passed on) and keeps the last results on screen
 * until the new ones arrive.
 */
export const searchQuery = (
  api: Api,
  params: SearchParams,
  limit = RESULTS_PER_PAGE,
  includeWeak = false,
) =>
  queryOptions({
    queryKey: searchKeys.results(params, limit, includeWeak),
    queryFn: ({ signal }) =>
      unwrap(
        api.GET("/api/v1/search", {
          params: {
            query: {
              ...toApiQuery({ ...params, q: params.q ?? "" }, limit),
              ...(includeWeak ? { include_weak: true } : {}),
            },
          },
          signal,
        }),
      ),
    enabled: Boolean(params.q),
    staleTime: 30_000,
    placeholderData: keepPreviousData,
    retry: retrySearch,
  });

/** Seconds to wait from a `rate_limited` error ("…, retry in 12s"), if it says. */
export function retryAfterSeconds(error: unknown): number | null {
  if (!isApiError(error)) return null;
  if (error.retryAfter) return error.retryAfter;
  const match = /retry in (\d+)\s*s/i.exec(error.message);
  return match ? Number(match[1]) : null;
}
