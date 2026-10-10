// The activity timeline and security log (`/api/v1/activity`), and sign-in
// sessions (`/api/v1/me/sessions`).

import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import { type Api, type Schemas, unwrap } from "./client";

export type ActivityItem = Schemas["ActivityItem"];
export type ActivityKind = Schemas["ActivityKind"];
export type ActivityCategory = Schemas["ActivityCategory"];
export type SessionInfo = Schemas["SessionResponse"];

export const activityKeys = {
  all: ["activity"] as const,
  list: (category?: ActivityCategory) => [...activityKeys.all, "list", category ?? "all"] as const,
  signIns: () => [...activityKeys.all, "sign-ins"] as const,
  sessions: ["me", "sessions"] as const,
};

const PAGE = 50;

export const activityQuery = (api: Api, category?: ActivityCategory) =>
  infiniteQueryOptions({
    queryKey: activityKeys.list(category),
    queryFn: ({ pageParam, signal }) =>
      unwrap(
        api.GET("/api/v1/activity", {
          params: {
            query: {
              limit: PAGE,
              ...(category ? { category } : {}),
              ...(pageParam ? { cursor: pageParam } : {}),
            },
          },
          signal,
        }),
      ),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor ?? null,
  });

/** The latest sign-ins and failed attempts (Settings → Security). */
export const signInsQuery = (api: Api) =>
  queryOptions({
    queryKey: activityKeys.signIns(),
    queryFn: ({ signal }) =>
      unwrap(
        api.GET("/api/v1/activity", {
          params: {
            query: { kind: "auth.signed_in,auth.sign_in_failed,account.created", limit: 8 },
          },
          signal,
        }),
      ),
  });

export const sessionsQuery = (api: Api) =>
  queryOptions({
    queryKey: activityKeys.sessions,
    queryFn: ({ signal }) => unwrap(api.GET("/api/v1/me/sessions", { signal })),
  });
