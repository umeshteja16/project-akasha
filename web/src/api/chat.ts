// Conversations and messages: types, keys and query options. Answers stream over
// SSE (`features/chat/stream.ts`); mutations live in `features/chat/mutations.ts`.

import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import { type Api, type Schemas, unwrap } from "./client";

export type Conversation = Schemas["ConversationResponse"];
export type Message = Schemas["MessageResponse"];
export type Citation = Schemas["Citation"];
export type AnswerStatus = Schemas["AnswerStatus"];
export type ChatSources = Schemas["ChatSources"];
export type ChatDelta = Schemas["ChatDelta"];
export type ChatDone = Schemas["ChatDone"];
export type ChatError = Schemas["ChatError"];

export const chatKeys = {
  all: ["chat"] as const,
  conversations: () => [...chatKeys.all, "conversations"] as const,
  conversation: (id: string) => [...chatKeys.all, "conversation", id] as const,
  messages: (id: string) => [...chatKeys.all, "messages", id] as const,
};

export const conversationsQuery = (api: Api) =>
  infiniteQueryOptions({
    queryKey: chatKeys.conversations(),
    queryFn: ({ pageParam, signal }) =>
      unwrap(
        api.GET("/api/v1/conversations", {
          params: { query: { limit: 30, ...(pageParam ? { cursor: pageParam } : {}) } },
          signal,
        }),
      ),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor ?? null,
  });

export const conversationQuery = (api: Api, id: string) =>
  queryOptions({
    queryKey: chatKeys.conversation(id),
    queryFn: ({ signal }) =>
      unwrap(api.GET("/api/v1/conversations/{id}", { params: { path: { id } }, signal })),
    retry: (failures, error) =>
      !(error instanceof Error && "status" in error && error.status === 404) && failures < 2,
  });

/** Pages go back in time (each oldest first); `pages` holds newest page first. */
export const messagesQuery = (api: Api, id: string) =>
  infiniteQueryOptions({
    queryKey: chatKeys.messages(id),
    queryFn: ({ pageParam, signal }) =>
      unwrap(
        api.GET("/api/v1/conversations/{id}/messages", {
          params: {
            path: { id },
            query: { limit: 50, ...(pageParam ? { cursor: pageParam } : {}) },
          },
          signal,
        }),
      ),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor ?? null,
    staleTime: 30_000,
  });

/** All loaded messages, oldest first. */
export function flattenMessages(pages: ReadonlyArray<{ items: Message[] }> | undefined): Message[] {
  if (!pages) return [];
  return [...pages].reverse().flatMap((p) => p.items);
}
