import { type InfiniteData, useMutation, useQueryClient } from "@tanstack/react-query";
import { type Conversation, chatKeys } from "@/api/chat";
import { unwrap } from "@/api/client";
import { useApi } from "@/api/context";

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
    mutationFn: () => unwrap(api.POST("/api/v1/conversations", { body: {} })),
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
