// Query keys and options shared by routes (loaders) and components.

import { queryOptions } from "@tanstack/react-query";
import { type Api, isApiError, type User, unwrap } from "./client";

export const keys = {
  me: ["me"] as const,
  meta: ["meta"] as const,
};

/** Fetch the signed-in user; `null` when not signed in (401). */
export async function fetchMe(api: Api): Promise<User | null> {
  try {
    return await unwrap(api.GET("/api/v1/me"));
  } catch (error) {
    if (isApiError(error) && error.status === 401) return null;
    throw error;
  }
}

/** The signed-in user, or `null`. Routes guard on it; sign-in/out set it directly. */
export const meQuery = (api: Api) =>
  queryOptions({
    queryKey: keys.me,
    queryFn: () => fetchMe(api),
    staleTime: 5 * 60_000,
    retry: false,
  });

/** Public server facts (registration open, version, chat model). */
export const metaQuery = (api: Api) =>
  queryOptions({
    queryKey: keys.meta,
    queryFn: () => unwrap(api.GET("/api/v1/meta")),
    staleTime: 10 * 60_000,
  });
