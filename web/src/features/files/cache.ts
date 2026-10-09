// Keeping cached file lists and details in step with changes made here.

import type { InfiniteData, QueryClient } from "@tanstack/react-query";
import type { Schemas } from "@/api/client";
import { type FileDetail, type FileItem, fileKeys } from "@/api/files";

type FileList = Schemas["FileList"];
type ListData = InfiniteData<FileList, string | null>;

function mapLists(queryClient: QueryClient, map: (items: FileItem[]) => FileItem[]) {
  queryClient.setQueriesData<ListData>({ queryKey: fileKeys.lists() }, (data) =>
    data ? { ...data, pages: data.pages.map((p) => ({ ...p, items: map(p.items) })) } : data,
  );
}

/** Show a just-uploaded file at the top of unfiltered newest-first lists right away. */
export function addUploadedFile(queryClient: QueryClient, file: FileItem) {
  for (const [key, data] of queryClient.getQueriesData<ListData>({ queryKey: fileKeys.lists() })) {
    const params = key[2] as { sort?: string; category?: string; tag?: string; pinned?: boolean };
    const unfiltered = !params?.category && !params?.tag && !params?.pinned;
    if (!data || !unfiltered || params?.sort !== "newest") continue;
    const [first, ...rest] = data.pages;
    if (!first || first.items.some((f) => f.id === file.id)) continue;
    queryClient.setQueryData<ListData>(key, {
      ...data,
      pages: [{ ...first, items: [file, ...first.items] }, ...rest],
    });
  }
  void queryClient.invalidateQueries({ queryKey: fileKeys.lists() });
  void queryClient.invalidateQueries({ queryKey: fileKeys.tags });
}

/** Replace a file everywhere it is cached (after an update). */
export function replaceFile(queryClient: QueryClient, file: FileItem) {
  mapLists(queryClient, (items) => items.map((f) => (f.id === file.id ? { ...f, ...file } : f)));
  queryClient.setQueryData<FileDetail>(fileKeys.detail(file.id), (old) =>
    old ? { ...old, ...file } : old,
  );
}

/** Drop deleted files from every cached list. */
export function removeFiles(queryClient: QueryClient, ids: readonly string[]) {
  const gone = new Set(ids);
  mapLists(queryClient, (items) => items.filter((f) => !gone.has(f.id)));
  for (const id of ids) queryClient.removeQueries({ queryKey: fileKeys.detail(id) });
}
