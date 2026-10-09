// Answers being streamed, one per conversation. A plain class read with
// `useSyncExternalStore` (like the upload queue): the stream outlives the page
// component (new chat → its conversation page) and is aborted when the chat area
// unmounts (`abortAll`) or the person presses stop.

import type { ChatDone, Citation } from "@/api/chat";
import type { Api } from "@/api/client";
import { isApiError } from "@/api/client";
import { isAbort, streamAnswer } from "./stream";

export type TurnPhase = "sending" | "streaming" | "done" | "stopped" | "failed";

export interface LiveTurn {
  /** Unique per ask (a retry is a new turn). */
  key: number;
  conversationId: string;
  question: string;
  fileIds: string[];
  phase: TurnPhase;
  /** Candidate passages from the `sources` event (numbered as the answer cites them). */
  sources: Citation[];
  /** Streamed text so far; after `done` the stored answer's content. */
  text: string;
  userMessageId?: string;
  done?: ChatDone;
  error?: { code: string; message: string };
}

export interface AskOptions {
  conversationId: string;
  question: string;
  fileIds?: string[];
  /** Called once the turn settles (done, failed or stopped). */
  onSettled?: (turn: LiveTurn) => void;
}

const MESSAGES: Record<string, string> = {
  rate_limited: "You're asking faster than this server allows. Wait a moment, then try again.",
  llm_rate_limited: "The language model is busy (rate limited). Try again in a moment.",
  llm_unavailable: "The language model can't be reached right now. Try again shortly.",
  llm_misconfigured: "The language model is misconfigured on this server.",
};

function errorOf(error: unknown): { code: string; message: string } {
  if (isApiError(error)) {
    return { code: error.code, message: MESSAGES[error.code] ?? error.message };
  }
  return { code: "internal", message: "Something went wrong while answering." };
}

export class ChatSessions {
  private turns = new Map<string, LiveTurn>();
  private controllers = new Map<string, AbortController>();
  private listeners = new Set<() => void>();
  private nextKey = 1;
  private snapshot: ReadonlyMap<string, LiveTurn> = new Map();

  constructor(private readonly api: Api) {}

  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): ReadonlyMap<string, LiveTurn> => this.snapshot;

  get(conversationId: string): LiveTurn | undefined {
    return this.turns.get(conversationId);
  }

  isBusy(conversationId: string): boolean {
    const phase = this.turns.get(conversationId)?.phase;
    return phase === "sending" || phase === "streaming";
  }

  /** Ask a question in a conversation (stopping an answer still running there). */
  ask({ conversationId, question, fileIds = [], onSettled }: AskOptions): void {
    this.stop(conversationId);
    const controller = new AbortController();
    this.controllers.set(conversationId, controller);
    const key = this.nextKey++;
    this.set({
      key,
      conversationId,
      question,
      fileIds,
      phase: "sending",
      sources: [],
      text: "",
    });
    const update = (changes: Partial<LiveTurn>) => {
      const turn = this.turns.get(conversationId);
      if (turn?.key === key) this.set({ ...turn, ...changes });
    };

    void streamAnswer(
      this.api,
      conversationId,
      { content: question, file_ids: fileIds },
      (event) => {
        const turn = this.turns.get(conversationId);
        if (turn?.key !== key) return;
        switch (event.type) {
          case "sources":
            update({
              phase: "streaming",
              sources: event.data.sources,
              userMessageId: event.data.user_message_id,
            });
            break;
          case "delta":
            update({ phase: "streaming", text: turn.text + event.data.text });
            break;
          case "done":
            update({ phase: "done", done: event.data, text: event.data.content });
            break;
          case "error":
            update({
              phase: "failed",
              error: {
                code: event.data.code,
                message: MESSAGES[event.data.code] ?? event.data.message,
              },
            });
            break;
        }
      },
      controller.signal,
    )
      .catch((error: unknown) => {
        if (isAbort(error) || controller.signal.aborted) update({ phase: "stopped" });
        else update({ phase: "failed", error: errorOf(error) });
      })
      .finally(() => {
        if (this.controllers.get(conversationId) === controller) {
          this.controllers.delete(conversationId);
        }
        const turn = this.turns.get(conversationId);
        if (turn?.key === key) onSettled?.(turn);
      });
  }

  /** Stop the answer streaming in a conversation (the server keeps it as cancelled). */
  stop(conversationId: string): void {
    this.controllers.get(conversationId)?.abort();
    this.controllers.delete(conversationId);
  }

  /** Forget a settled turn (once the stored messages show it). */
  clear(conversationId: string, key?: number): void {
    const turn = this.turns.get(conversationId);
    if (!turn || (key !== undefined && turn.key !== key)) return;
    if (turn.phase === "sending" || turn.phase === "streaming") return;
    this.turns.delete(conversationId);
    this.emit();
  }

  abortAll(): void {
    for (const controller of this.controllers.values()) controller.abort();
    this.controllers.clear();
  }

  private set(turn: LiveTurn): void {
    this.turns.set(turn.conversationId, turn);
    this.emit();
  }

  private emit(): void {
    this.snapshot = new Map(this.turns);
    for (const listener of this.listeners) listener();
  }
}
