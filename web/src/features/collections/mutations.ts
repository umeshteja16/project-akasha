// Collection mutations. Every change refreshes the collection list (sidebar),
// the collection itself, file lists (collection pages) and file details (chips).

import { type QueryClient, useMutation, useQueryClient } from "@tanstack/react-query";
import { unwrap } from "@/api/client";
import {
  type Collection,
  type CollectionColor,
  type CollectionIcon,
  collectionKeys,
} from "@/api/collections";
import { useApi } from "@/api/context";
import { fileKeys } from "@/api/files";

export interface CollectionFields {
  name: string;
  description: string;
  color: CollectionColor;
  icon: CollectionIcon;
}

function refresh(queryClient: QueryClient, collection?: Collection) {
  void queryClient.invalidateQueries({ queryKey: collectionKeys.list() });
  if (collection) queryClient.setQueryData(collectionKeys.detail(collection.id), collection);
}

function refreshFiles(queryClient: QueryClient, ids: readonly string[]) {
  void queryClient.invalidateQueries({ queryKey: fileKeys.lists() });
  for (const id of ids) void queryClient.invalidateQueries({ queryKey: fileKeys.detail(id) });
}

export function useCreateCollection() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: CollectionFields & { file_ids?: string[] }) =>
      unwrap(api.POST("/api/v1/collections", { body })),
    onSuccess: (created, body) => {
      refresh(queryClient, created);
      if (body.file_ids?.length) refreshFiles(queryClient, body.file_ids);
    },
  });
}

export function useUpdateCollection(id: string) {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: Partial<CollectionFields>) =>
      unwrap(api.PATCH("/api/v1/collections/{id}", { params: { path: { id } }, body })),
    onSuccess: (updated) => {
      refresh(queryClient, updated);
      // File pages show the collection's name and look.
      void queryClient.invalidateQueries({ queryKey: [...fileKeys.all, "detail"] });
    },
  });
}

export function useDeleteCollection() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.DELETE("/api/v1/collections/{id}", { params: { path: { id } } })),
    onSuccess: (_, id) => {
      queryClient.removeQueries({ queryKey: collectionKeys.detail(id) });
      void queryClient.invalidateQueries({ queryKey: collectionKeys.list() });
      void queryClient.invalidateQueries({ queryKey: [...fileKeys.all, "detail"] });
    },
  });
}

/** Add files to (or, with `remove`, take them out of) a collection. */
export function useCollectionFiles() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, fileIds, remove }: { id: string; fileIds: string[]; remove?: boolean }) =>
      unwrap(
        remove
          ? api.POST("/api/v1/collections/{id}/files/remove", {
              params: { path: { id } },
              body: { file_ids: fileIds },
            })
          : api.POST("/api/v1/collections/{id}/files", {
              params: { path: { id } },
              body: { file_ids: fileIds },
            }),
      ),
    onSuccess: (res, { fileIds }) => {
      refresh(queryClient, res.collection);
      refreshFiles(queryClient, fileIds);
    },
  });
}
