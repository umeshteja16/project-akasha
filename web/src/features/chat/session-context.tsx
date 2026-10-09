import { useQueryClient } from "@tanstack/react-query";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useState,
  useSyncExternalStore,
} from "react";
import { chatKeys } from "@/api/chat";
import { useApi } from "@/api/context";
import { type AskOptions, ChatSessions, type LiveTurn } from "./chat-session";

const SessionsContext = createContext<ChatSessions | null>(null);

/** Holds the streaming answers of the chat area; leaving it stops them. */
export function ChatSessionProvider({ children }: { children: ReactNode }) {
  const api = useApi();
  const [sessions] = useState(() => new ChatSessions(api));
  useEffect(() => () => sessions.abortAll(), [sessions]);
  return <SessionsContext.Provider value={sessions}>{children}</SessionsContext.Provider>;
}

export function useChatSessions(): ChatSessions {
  const sessions = useContext(SessionsContext);
  if (!sessions) throw new Error("useChatSessions must be used inside <ChatSessionProvider>");
  return sessions;
}

export function useLiveTurn(conversationId: string): LiveTurn | undefined {
  const sessions = useChatSessions();
  const turns = useSyncExternalStore(sessions.subscribe, sessions.getSnapshot);
  return turns.get(conversationId);
}

const sleep = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms));

/**
 * Ask in a conversation. When the answer settles the stored messages are reloaded
 * (then the live turn is dropped), and after a conversation's first answer the list
 * is refreshed a few times to pick up the model-written title.
 */
export function useAsk() {
  const sessions = useChatSessions();
  const queryClient = useQueryClient();
  return useCallback(
    (options: Omit<AskOptions, "onSettled"> & { first?: boolean }) => {
      const { conversationId: id, first } = options;
      sessions.ask({
        ...options,
        onSettled: (turn) => {
          void (async () => {
            // A stopped answer is stored once the server notices the disconnect.
            if (turn.phase === "stopped") await sleep(700);
            await queryClient.invalidateQueries({ queryKey: chatKeys.messages(id) });
            if (turn.phase === "done" || turn.phase === "stopped") sessions.clear(id, turn.key);
            void queryClient.invalidateQueries({ queryKey: chatKeys.conversations() });
            if (first && turn.phase === "done") {
              for (const ms of [2500, 6000]) {
                window.setTimeout(() => {
                  void queryClient.invalidateQueries({ queryKey: chatKeys.conversations() });
                  void queryClient.invalidateQueries({ queryKey: chatKeys.conversation(id) });
                }, ms);
              }
            }
          })();
        },
      });
    },
    [sessions, queryClient],
  );
}
