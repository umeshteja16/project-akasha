import { useQuery } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";
import { TriangleAlertIcon } from "lucide-react";
import { isApiError } from "@/api/client";
import { useApi } from "@/api/context";
import { metaQuery } from "@/api/queries";
import { toast } from "@/components/ui/toast";
import { ChatTopBar } from "@/features/chat/chat-top-bar";
import { Composer } from "@/features/chat/composer";
import { useCreateConversation } from "@/features/chat/mutations";
import { CollectionScopeBar, parseFileScope, ScopeBar } from "@/features/chat/scope";
import { useAsk } from "@/features/chat/session-context";
import { useDocumentTitle } from "@/lib/use-document-title";

const route = getRouteApi("/app/chat/");

const SUGGESTIONS = [
  "Which of my documents mention a renewal date?",
  "Summarise what my notes say about the budget",
  "What did I write down about the trip?",
];

/** A new conversation: what to ask, then the composer. */
export function ChatHome() {
  useDocumentTitle("New conversation");
  const api = useApi();
  const navigate = route.useNavigate();
  const { files, collection } = route.useSearch();
  const fileIds = parseFileScope(files);
  const chatModel = useQuery(metaQuery(api)).data?.chat_model;
  const create = useCreateConversation();
  const ask = useAsk();

  const send = (question: string) => {
    create.mutate(
      { fileIds, collectionId: collection },
      {
        onSuccess: (conversation) => {
          // The scope is stored with the conversation from here on.
          ask({ conversationId: conversation.id, question, fileIds, first: true });
          void navigate({
            to: "/chat/$conversationId",
            params: { conversationId: conversation.id },
            search: {},
          });
        },
        onError: (e) =>
          toast({
            title: "Couldn't start a conversation",
            description: isApiError(e) ? e.message : undefined,
            tone: "danger",
          }),
      },
    );
  };

  return (
    <>
      <ChatTopBar title="New conversation" />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto grid max-w-[44rem] gap-8 px-4 pt-12 pb-8 sm:px-6 md:pt-[14vh]">
          <div className="grid animate-fade-in gap-3">
            <p className="eyebrow">Ask</p>
            <h2 className="display text-3xl text-fg sm:text-4xl">What would you like to know?</h2>
            <p className="max-w-[var(--reading-max)] text-base text-fg-muted">
              Answers come only from your files, with every claim linked to the passage it came
              from. When the evidence is thin, Akasha says so instead of guessing.
            </p>
          </div>
          {chatModel === false ? (
            <p className="flex items-start gap-2 rounded-lg border border-border bg-surface-2/50 px-4 py-3 text-sm text-fg-muted">
              <TriangleAlertIcon className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden />
              <span>
                This server has no language model configured, so answers will be the most relevant
                passages themselves. Set{" "}
                <code className="font-mono text-xs">AKASHA_LLM_PROVIDER</code> to get written
                answers.
              </span>
            </p>
          ) : null}
          <ul className="grid gap-2 sm:grid-cols-3" aria-label="Suggested questions">
            {SUGGESTIONS.map((s) => (
              <li key={s}>
                <button
                  type="button"
                  disabled={create.isPending}
                  onClick={() => send(s)}
                  className="h-full w-full rounded-lg border border-border bg-surface px-3.5 py-3 text-left text-sm text-fg-muted shadow-xs transition-colors hover:border-border-strong hover:text-fg"
                >
                  {s}
                </button>
              </li>
            ))}
          </ul>
        </div>
      </div>
      <div className="mx-auto w-full max-w-[46rem] shrink-0 px-3 pb-3 sm:px-6 sm:pb-5">
        <Composer
          autoFocus
          onSend={send}
          disabled={create.isPending}
          top={
            collection ? (
              <CollectionScopeBar
                collectionId={collection}
                onClear={() => void navigate({ search: {}, replace: true })}
              />
            ) : fileIds.length ? (
              <ScopeBar
                fileIds={fileIds}
                onClear={() => void navigate({ search: {}, replace: true })}
              />
            ) : null
          }
        />
      </div>
    </>
  );
}
