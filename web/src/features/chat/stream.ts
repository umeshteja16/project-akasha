// Ask a question and stream the answer: `POST /api/v1/conversations/{id}/messages`
// answers with `text/event-stream` (events `sources`, `delta`, `done`, `error`).
// `fetch` + a streamed body, because EventSource cannot POST.

import type { ChatDelta, ChatDone, ChatError, ChatSources } from "@/api/chat";
import { type Api, ApiError, unwrap } from "@/api/client";
import { readSse, type SseMessage } from "@/lib/sse";

export type ChatEvent =
  | { type: "sources"; data: ChatSources }
  | { type: "delta"; data: ChatDelta }
  | { type: "done"; data: ChatDone }
  | { type: "error"; data: ChatError };

const KNOWN = new Set(["sources", "delta", "done", "error"]);

/** A typed chat event, `null` for events this client doesn't know (ignored). */
export function decodeChatEvent(message: SseMessage): ChatEvent | null {
  if (!KNOWN.has(message.event)) return null;
  let data: unknown;
  try {
    data = JSON.parse(message.data);
  } catch {
    throw new ApiError(0, "bad_stream", "The answer arrived garbled. Try again.");
  }
  if (typeof data !== "object" || data === null) {
    throw new ApiError(0, "bad_stream", "The answer arrived garbled. Try again.");
  }
  return { type: message.event, data } as ChatEvent;
}

export interface AskBody {
  content: string;
  file_ids?: string[];
}

/**
 * Post a question and call `onEvent` for each streamed event. Resolves when the
 * stream ends with `done` or `error`; rejects with an `ApiError` (HTTP errors such
 * as `rate_limited`, a lost connection: `stream_interrupted`) or an `AbortError`.
 */
export async function streamAnswer(
  api: Api,
  conversationId: string,
  body: AskBody,
  onEvent: (event: ChatEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const stream = await unwrap(
    api.POST("/api/v1/conversations/{id}/messages", {
      params: { path: { id: conversationId } },
      body: {
        content: body.content,
        ...(body.file_ids?.length ? { file_ids: body.file_ids } : {}),
      },
      headers: { accept: "text/event-stream" },
      parseAs: "stream",
      signal,
    }),
  );
  if (!stream) throw new ApiError(0, "stream_interrupted", "The answer didn't arrive.");
  let finished = false;
  try {
    await readSse(
      stream,
      (message) => {
        const event = decodeChatEvent(message);
        if (!event) return;
        if (event.type === "done" || event.type === "error") finished = true;
        onEvent(event);
      },
      signal,
    );
  } catch (error) {
    if (signal?.aborted) throw error;
    if (error instanceof ApiError) throw error;
    throw new ApiError(
      0,
      "stream_interrupted",
      "The connection dropped before the answer finished.",
    );
  }
  if (!finished && !signal?.aborted) {
    throw new ApiError(
      0,
      "stream_interrupted",
      "The connection dropped before the answer finished.",
    );
  }
}

export function isAbort(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}
