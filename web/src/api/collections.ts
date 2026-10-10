// Collections: named groups of files (`/api/v1/collections`). A collection's files
// are the files list filtered by `collection_id` (see `filesQuery`).

import { queryOptions } from "@tanstack/react-query";
import { type Api, type Schemas, unwrap } from "./client";

export type Collection = Schemas["CollectionResponse"];
export type CollectionSummary = Schemas["CollectionSummary"];
export type CollectionColor = Schemas["CollectionColor"];
export type CollectionIcon = Schemas["CollectionIcon"];

export const collectionKeys = {
  all: ["collections"] as const,
  list: () => [...collectionKeys.all, "list"] as const,
  detail: (id: string) => [...collectionKeys.all, "detail", id] as const,
};

export const collectionsQuery = (api: Api) =>
  queryOptions({
    queryKey: collectionKeys.list(),
    queryFn: ({ signal }) => unwrap(api.GET("/api/v1/collections", { signal })),
    staleTime: 60_000,
  });

export const collectionQuery = (api: Api, id: string) =>
  queryOptions({
    queryKey: collectionKeys.detail(id),
    queryFn: ({ signal }) =>
      unwrap(api.GET("/api/v1/collections/{id}", { params: { path: { id } }, signal })),
  });
