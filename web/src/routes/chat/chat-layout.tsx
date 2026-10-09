import { Outlet } from "@tanstack/react-router";
import { ConversationList } from "@/features/chat/conversation-list";
import { ChatSessionProvider } from "@/features/chat/session-context";

/**
 * The chat area: conversation list (desktop sidebar; a sheet on phones) beside the
 * open conversation. Fills the viewport; only the messages scroll. Leaving the chat
 * area stops any answer still streaming.
 */
export function ChatLayout() {
  return (
    <ChatSessionProvider>
      <div className="grid h-[calc(100dvh-7.5rem-env(safe-area-inset-bottom))] md:h-dvh md:grid-cols-[16.5rem_minmax(0,1fr)]">
        <aside
          aria-label="Conversation list"
          className="hidden min-h-0 flex-col border-r border-border md:flex"
        >
          <ConversationList />
        </aside>
        <div className="flex min-h-0 min-w-0 flex-col">
          <Outlet />
        </div>
      </div>
    </ChatSessionProvider>
  );
}
