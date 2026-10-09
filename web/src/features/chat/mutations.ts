import { type InfiniteData, useMutation, useQueryClient } from "@tanstack/react-query";
import { type Conversation, chatKeys } from "@/api/chat";
import { unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { toast } from "@/components/ui/toast";

type ConversationPages = InfiniteData<{ items: Conversation[]; next_cursor?: string | null }>;

function mapPages(
  data: ConversationPages | undefined,
  map: (items: Conversation[]) => Conversation[],
): ConversationPages | undefined {
  if (!data) return data;
  return { ...data, pages: data.pages.map((p) => ({ ...p, items: map(p.items) })) };
}

export function useCreateConversation() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ fileIds = [] }: { fileIds?: string[] } = {}) =>
      unwrap(
        api.POST("/api/v1/conversations", {
          body: fileIds.length ? { file_ids: fileIds } : {},
        }),
      ),
    onSuccess: (created) => {
      queryClient.setQueryData(chatKeys.conversation(created.id), created);
      queryClient.setQueryData<ConversationPages>(chatKeys.conversations(), (data) => {
        if (!data || data.pages.length === 0) return data;
        const [first, ...rest] = data.pages;
        return first
          ? { ...data, pages: [{ ...first, items: [created, ...first.items] }, ...rest] }
          : data;
      });
    },
  });
}

export function useRenameConversation() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, title }: { id: string; title: string }) =>
      unwrap(
        api.PATCH("/api/v1/conversations/{id}", { params: { path: { id } }, body: { title } }),
      ),
    onSuccess: (updated) => {
      queryClient.setQueryData(chatKeys.conversation(updated.id), updated);
      queryClient.setQueryData<ConversationPages>(chatKeys.conversations(), (data) =>
        mapPages(data, (items) => items.map((c) => (c.id === updated.id ? updated : c))),
      );
    },
  });
}

export function useDeleteConversation() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.DELETE("/api/v1/conversations/{id}", { params: { path: { id } } })),
    onSuccess: (_data, id) => {
      queryClient.setQueryData<ConversationPages>(chatKeys.conversations(), (data) =>
        mapPages(data, (items) => items.filter((c) => c.id !== id)),
      );
      queryClient.removeQueries({ queryKey: chatKeys.conversation(id) });
      queryClient.removeQueries({ queryKey: chatKeys.messages(id) });
    },
  });
}

/**
 * Change the files a conversation answers from (`[]`: the whole library). The
 * scope bar updates at once and rolls back if the server refuses.
 */
export function useSetConversationScope() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, fileIds }: { id: string; fileIds: string[] }) =>
      unwrap(
        api.PATCH("/api/v1/conversations/{id}", {
          params: { path: { id } },
          body: { file_ids: fileIds },
        }),
      ),
    onMutate: ({ id, fileIds }) => {
      const key = chatKeys.conversation(id);
      const before = queryClient.getQueryData<Conversation>(key);
      if (before) queryClient.setQueryData<Conversation>(key, { ...before, file_ids: fileIds });
      return { before };
    },
    onError: (_error, { id }, context) => {
      if (context?.before) queryClient.setQueryData(chatKeys.conversation(id), context.before);
      toast({ title: "Couldn't change which files this chat uses", tone: "danger" });
    },
    onSuccess: (updated) => {
      queryClient.setQueryData(chatKeys.conversation(updated.id), updated);
    },
  });
}
