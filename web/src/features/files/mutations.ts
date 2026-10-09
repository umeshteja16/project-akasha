// File mutations: update (rename, pin, tags), delete, reindex, re-enrich.

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type Api, isApiError, unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { type FileItem, fileKeys } from "@/api/files";
import { toast } from "@/components/ui/toast";
import { removeFiles, replaceFile } from "./cache";

export interface FileChanges {
  name?: string;
  is_pinned?: boolean;
  tags?: string[];
  auto_tags?: string[];
}

export function useUpdateFile() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, changes }: { id: string; changes: FileChanges }) =>
      unwrap(api.PATCH("/api/v1/files/{id}", { params: { path: { id } }, body: changes })),
    onSuccess: (file, { changes }) => {
      replaceFile(queryClient, file);
      if (changes.tags || changes.auto_tags) {
        void queryClient.invalidateQueries({ queryKey: fileKeys.tags });
      }
      // Pin and tag filters may no longer match.
      if (changes.is_pinned !== undefined || changes.tags || changes.auto_tags) {
        void queryClient.invalidateQueries({ queryKey: fileKeys.lists() });
      }
    },
  });
}

/** Toggle a pin with an instant visual change and a rollback on failure. */
export function useTogglePin() {
  const update = useUpdateFile();
  const queryClient = useQueryClient();
  return (file: FileItem) => {
    const next = !file.is_pinned;
    replaceFile(queryClient, { ...file, is_pinned: next });
    update.mutate(
      { id: file.id, changes: { is_pinned: next } },
      {
        onError: () => {
          replaceFile(queryClient, file);
          toast({ title: "Couldn't change the pin", tone: "danger" });
        },
      },
    );
  };
}

const BULK_LIMIT = 100;

async function deleteFiles(api: Api, ids: readonly string[]): Promise<string[]> {
  if (ids.length === 1 && ids[0]) {
    await unwrap(api.DELETE("/api/v1/files/{id}", { params: { path: { id: ids[0] } } }));
    return [ids[0]];
  }
  const deleted: string[] = [];
  for (let i = 0; i < ids.length; i += BULK_LIMIT) {
    const batch = ids.slice(i, i + BULK_LIMIT);
    const res = await unwrap(api.POST("/api/v1/files/bulk-delete", { body: { ids: batch } }));
    deleted.push(...res.deleted);
  }
  return deleted;
}

export function useDeleteFiles() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (ids: readonly string[]) => deleteFiles(api, ids),
    onSuccess: (deleted) => {
      removeFiles(queryClient, deleted);
      void queryClient.invalidateQueries({ queryKey: fileKeys.lists() });
      void queryClient.invalidateQueries({ queryKey: fileKeys.tags });
    },
  });
}

export function useReindex() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.POST("/api/v1/files/{id}/reindex", { params: { path: { id } } })),
    onSuccess: (detail) => {
      queryClient.setQueryData(fileKeys.detail(detail.id), detail);
      replaceFile(queryClient, detail);
      void queryClient.invalidateQueries({ queryKey: fileKeys.extraction(detail.id) });
      void queryClient.invalidateQueries({ queryKey: fileKeys.similar(detail.id) });
      void queryClient.invalidateQueries({ queryKey: fileKeys.lists() });
    },
  });
}

export function useEnrich() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.POST("/api/v1/files/{id}/enrich", { params: { path: { id } } })),
    onSuccess: (detail) => {
      queryClient.setQueryData(fileKeys.detail(detail.id), detail);
    },
  });
}

/** Plain-words message for a failed re-enrichment. */
export function enrichErrorMessage(error: unknown): string {
  if (!isApiError(error)) return "Something went wrong. Try again.";
  if (error.status === 409) return "The file is still being read. Try again once it's ready.";
  if (error.status === 503) return "No language model is configured on this server.";
  if (error.status === 429) return "Too many requests. Wait a minute and try again.";
  return error.message;
}
