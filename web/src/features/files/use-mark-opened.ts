import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { fileKeys } from "@/api/files";
import { replaceFile } from "./cache";

/**
 * Tell the server this file was opened ("Recently opened", activity), once per
 * file page visit. Failures are ignored: it is bookkeeping, not the user's task.
 */
export function useMarkOpened(fileId: string) {
  const api = useApi();
  const queryClient = useQueryClient();
  const sent = useRef<string | null>(null);
  useEffect(() => {
    if (sent.current === fileId) return;
    sent.current = fileId;
    unwrap(api.POST("/api/v1/files/{id}/open", { params: { path: { id: fileId } } }))
      .then((file) => {
        replaceFile(queryClient, file);
        // "Recently opened" lists change order.
        void queryClient.invalidateQueries({
          queryKey: fileKeys.lists(),
          predicate: (q) => (q.queryKey[2] as { sort?: string } | undefined)?.sort === "opened",
        });
      })
      .catch(() => undefined);
  }, [api, fileId, queryClient]);
}
