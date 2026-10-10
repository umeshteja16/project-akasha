import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { getRouteApi, Link } from "@tanstack/react-router";
import { EllipsisIcon, MessageSquareOffIcon, PencilIcon, Trash2Icon } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import {
  type Conversation,
  conversationQuery,
  flattenMessages,
  type Message,
  messagesQuery,
} from "@/api/chat";
import { isApiError } from "@/api/client";
import { useApi } from "@/api/context";
import { EmptyState } from "@/components/common/empty-state";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip } from "@/components/ui/tooltip";
import { Answer, type AnswerView } from "@/features/chat/answer";
import { ChatScroll } from "@/features/chat/chat-scroll";
import type { LiveTurn } from "@/features/chat/chat-session";
import { ChatTopBar } from "@/features/chat/chat-top-bar";
import { Composer } from "@/features/chat/composer";
import {
  conversationTitle,
  DeleteConversationDialog,
  RenameConversationDialog,
} from "@/features/chat/conversation-dialogs";
import { useClearConversationCollection, useSetConversationScope } from "@/features/chat/mutations";
import { CollectionScopeBar, parseFileScope, ScopeBar } from "@/features/chat/scope";
import { useAsk, useChatSessions, useLiveTurn } from "@/features/chat/session-context";
import { useDocumentTitle } from "@/lib/use-document-title";

const route = getRouteApi("/app/chat/$conversationId");

export function ConversationPage() {
  const { conversationId } = route.useParams();
  // Keyed: switching conversations starts from a fresh view (scroll, dialogs).
  return <ConversationView key={conversationId} id={conversationId} />;
}

function liveView(turn: LiveTurn): AnswerView {
  const done = turn.done;
  const state =
    turn.phase === "sending" || turn.phase === "streaming"
      ? turn.phase
      : turn.phase === "stopped"
        ? "cancelled"
        : turn.phase === "failed"
          ? "error"
          : (done?.status ?? "answered");
  const cited = done?.citations ?? [];
  const byN = new Map([...turn.sources, ...cited].map((c) => [c.n, c]));
  return {
    text: turn.text,
    state,
    citable: [...byN.values()],
    cited,
    model: done?.model,
    latencyMs: done?.latency_ms,
    error: turn.error?.message,
  };
}

function storedView(m: Message): AnswerView {
  return {
    text: m.content,
    state: m.status,
    citable: m.citations,
    cited: m.citations,
    model: m.model,
    latencyMs: m.latency_ms,
    error: m.status === "error" ? "The answer was interrupted by an error." : undefined,
  };
}

function ConversationView({ id }: { id: string }) {
  const api = useApi();
  const navigate = route.useNavigate();
  const { files } = route.useSearch();
  const conversation = useQuery(conversationQuery(api, id));
  useDocumentTitle(
    conversation.isError
      ? "Conversation not found"
      : conversation.data
        ? conversationTitle(conversation.data)
        : "Chat",
  );
  const setScope = useSetConversationScope();
  const clearCollection = useClearConversationCollection();
  // The scope is stored with the conversation; `?files=` (an older link) sets it.
  const fileIds = conversation.data?.file_ids ?? [];
  const linked = parseFileScope(files).join(",");
  const loaded = conversation.isSuccess;
  const { mutate: saveScope } = setScope;
  useEffect(() => {
    if (!loaded || !linked) return;
    saveScope({ id, fileIds: linked.split(",") });
    void navigate({ search: {}, replace: true });
  }, [loaded, linked, id, saveScope, navigate]);
  const messages = useInfiniteQuery(messagesQuery(api, id));
  const turn = useLiveTurn(id);
  const sessions = useChatSessions();
  const ask = useAsk();
  const [renaming, setRenaming] = useState<Conversation | null>(null);
  const [deleting, setDeleting] = useState<Conversation | null>(null);
  const [draft, setDraft] = useState<{ text: string; key: number }>();
  const edit = (text: string) => setDraft({ text, key: Date.now() });

  const hidden = new Set(
    [turn?.userMessageId, turn?.done?.message_id].filter((x): x is string => Boolean(x)),
  );
  const history = flattenMessages(messages.data?.pages).filter((m) => !hidden.has(m.id));
  const busy = turn?.phase === "sending" || turn?.phase === "streaming";

  const send = (question: string) =>
    ask({
      conversationId: id,
      question,
      fileIds,
      first: history.length === 0 && !turn,
    });

  if (conversation.isError) {
    const missing =
      isApiError(conversation.error) &&
      (conversation.error.status === 404 || conversation.error.status === 400);
    return (
      <>
        <ChatTopBar title="Chat" />
        <EmptyState
          icon={MessageSquareOffIcon}
          title={missing ? "This conversation isn't here" : "Couldn't open this conversation"}
          actions={
            <Button variant="secondary" asChild>
              <Link to="/chat" search={{}}>
                Start a new one
              </Link>
            </Button>
          }
        >
          <p>{missing ? "It may have been deleted." : "Try again in a moment."}</p>
        </EmptyState>
      </>
    );
  }

  // The last stored answer can be asked again (when nothing is streaming).
  const lastAnswer = history.at(-1)?.role === "assistant" ? history.at(-1) : undefined;
  let question = "";

  return (
    <>
      <ChatTopBar
        title={
          conversation.data ? (
            conversationTitle(conversation.data)
          ) : (
            <Skeleton className="h-5 w-48" />
          )
        }
        actions={
          conversation.data ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon-sm" aria-label="Conversation actions">
                  <EllipsisIcon />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onSelect={() => setRenaming(conversation.data)}>
                  <PencilIcon /> Rename
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem tone="danger" onSelect={() => setDeleting(conversation.data)}>
                  <Trash2Icon /> Delete…
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : null
        }
      />

      <ChatScroll followKey={`${turn?.key ?? 0}-${messages.isSuccess}`}>
        <div className="mx-auto grid max-w-[46rem] gap-8 px-4 pt-8 pb-10 sm:px-6">
          {messages.hasNextPage ? (
            <div className="flex justify-center">
              <Button
                variant="ghost"
                size="sm"
                disabled={messages.isFetchingNextPage}
                onClick={() => void messages.fetchNextPage()}
              >
                {messages.isFetchingNextPage ? "Loading…" : "Earlier messages"}
              </Button>
            </div>
          ) : null}

          {messages.isPending ? (
            <div className="grid gap-6" role="status" aria-label="Loading messages">
              <Skeleton className="ml-auto h-10 w-2/3 rounded-xl" />
              <div className="grid gap-2">
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-11/12" />
                <Skeleton className="h-4 w-3/5" />
              </div>
            </div>
          ) : messages.isError ? (
            <p className="text-sm text-danger">Couldn't load the messages.</p>
          ) : null}

          {history.map((m) => {
            if (m.role === "user") {
              question = m.content;
              return <UserMessage key={m.id} text={m.content} onEdit={busy ? undefined : edit} />;
            }
            const asked = question;
            const isLast = m === lastAnswer && !turn;
            return (
              <AssistantBlock key={m.id}>
                <Answer
                  view={storedView(m)}
                  question={asked}
                  onRetry={isLast && asked ? () => send(asked) : undefined}
                />
              </AssistantBlock>
            );
          })}

          {turn ? (
            <>
              <UserMessage text={turn.question} />
              <AssistantBlock>
                <Answer
                  view={liveView(turn)}
                  question={turn.question}
                  onStop={() => sessions.stop(id)}
                  onRetry={busy ? undefined : () => send(turn.question)}
                />
              </AssistantBlock>
            </>
          ) : null}

          {messages.isSuccess && history.length === 0 && !turn ? (
            <p className="py-10 text-center text-sm text-fg-subtle">
              Ask the first question below.
            </p>
          ) : null}
        </div>
      </ChatScroll>

      <div className="mx-auto w-full max-w-[46rem] shrink-0 px-3 pb-3 sm:px-6 sm:pb-5">
        <Composer
          autoFocus
          streaming={busy}
          onSend={send}
          onStop={() => sessions.stop(id)}
          draft={draft}
          placeholder={history.length || turn ? "Ask a follow-up…" : "Ask about your files…"}
          top={
            conversation.data?.collection_id ? (
              <CollectionScopeBar
                collectionId={conversation.data.collection_id}
                onClear={() => clearCollection.mutate(id)}
              />
            ) : fileIds.length ? (
              <ScopeBar fileIds={fileIds} onClear={() => setScope.mutate({ id, fileIds: [] })} />
            ) : null
          }
        />
      </div>

      <RenameConversationDialog
        conversation={renaming}
        onOpenChange={(open) => !open && setRenaming(null)}
      />
      <DeleteConversationDialog
        conversation={deleting}
        onOpenChange={(open) => !open && setDeleting(null)}
        onDeleted={() => {
          sessions.stop(id);
          void navigate({ to: "/chat", search: {}, replace: true });
        }}
      />
    </>
  );
}

function UserMessage({ text, onEdit }: { text: string; onEdit?: (text: string) => void }) {
  return (
    <div className="group/question flex animate-fade-in items-start justify-end gap-1">
      {onEdit ? (
        <Tooltip content="Edit and ask again">
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Edit and ask again"
            className="mt-1 text-fg-subtle opacity-100 transition-opacity sm:opacity-0 sm:group-hover/question:opacity-100 sm:focus-visible:opacity-100"
            onClick={() => onEdit(text)}
          >
            <PencilIcon />
          </Button>
        </Tooltip>
      ) : null}
      <p className="max-w-[85%] rounded-xl rounded-br-sm bg-surface-2 px-4 py-2.5 text-base whitespace-pre-wrap text-fg [overflow-wrap:anywhere]">
        <span className="sr-only">You asked: </span>
        {text}
      </p>
    </div>
  );
}

function AssistantBlock({ children }: { children: ReactNode }) {
  return (
    <article aria-label="Answer" className="grid animate-fade-in gap-2">
      <p className="eyebrow flex items-center gap-2">
        <span className="size-1.5 rounded-full bg-highlight" aria-hidden />
        Akasha
      </p>
      {children}
    </article>
  );
}
