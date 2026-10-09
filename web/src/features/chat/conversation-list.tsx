import { useInfiniteQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { EllipsisIcon, PencilIcon, SquarePenIcon, Trash2Icon } from "lucide-react";
import { useState, useSyncExternalStore } from "react";
import { type Conversation, conversationsQuery } from "@/api/chat";
import { useApi } from "@/api/context";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import { formatRelative } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  conversationTitle,
  DeleteConversationDialog,
  RenameConversationDialog,
} from "./conversation-dialogs";
import { useChatSessions } from "./session-context";

/** Conversations, most recent first, with rename and delete. */
export function ConversationList({ onNavigate }: { onNavigate?: () => void }) {
  const api = useApi();
  const navigate = useNavigate();
  const query = useInfiniteQuery(conversationsQuery(api));
  const current = useParams({ strict: false }).conversationId;
  const sessions = useChatSessions();
  const live = useSyncExternalStore(sessions.subscribe, sessions.getSnapshot);
  const [renaming, setRenaming] = useState<Conversation | null>(null);
  const [deleting, setDeleting] = useState<Conversation | null>(null);
  const items = query.data?.pages.flatMap((p) => p.items) ?? [];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div
        className={cn(
          "flex items-center justify-between gap-2 px-4 pt-5 pb-3",
          onNavigate && "pr-12",
        )}
      >
        <h2 className="eyebrow">Conversations</h2>
        <Button variant="secondary" size="sm" asChild>
          <Link to="/chat" search={{}} onClick={onNavigate}>
            <SquarePenIcon /> New chat
          </Link>
        </Button>
      </div>
      <nav aria-label="Conversations" className="min-h-0 flex-1 overflow-y-auto px-2 pb-4">
        {query.isPending ? (
          <div className="grid gap-2 px-2 pt-1" role="status" aria-label="Loading conversations">
            {[0, 1, 2, 3].map((i) => (
              <Skeleton key={i} className="h-9" />
            ))}
          </div>
        ) : query.isError ? (
          <p className="px-3 py-2 text-xs text-danger">Couldn't load conversations.</p>
        ) : items.length === 0 ? (
          <p className="px-3 py-2 text-xs text-fg-subtle">
            Nothing asked yet. Your conversations appear here.
          </p>
        ) : (
          <ul className="grid gap-0.5">
            {items.map((c) => {
              const active = c.id === current;
              const busy = ["sending", "streaming"].includes(live.get(c.id)?.phase ?? "");
              return (
                <li
                  key={c.id}
                  className={cn(
                    "group/item relative flex items-center rounded-md pr-1 transition-colors",
                    active ? "bg-surface shadow-xs" : "hover:bg-surface-2",
                  )}
                >
                  <Link
                    to="/chat/$conversationId"
                    params={{ conversationId: c.id }}
                    search={{}}
                    onClick={onNavigate}
                    aria-current={active ? "page" : undefined}
                    className={cn(
                      "grid min-w-0 flex-1 gap-0.5 rounded-md py-2 pr-1 pl-3",
                      active ? "" : "text-fg-muted hover:text-fg",
                    )}
                  >
                    {active ? (
                      <span
                        aria-hidden
                        className="absolute top-2 bottom-2 left-0 w-0.5 rounded-full bg-accent"
                      />
                    ) : null}
                    <span className={cn("truncate text-sm", active ? "font-medium text-fg" : "")}>
                      {conversationTitle(c)}
                    </span>
                    <span className="flex items-center gap-1.5 text-2xs text-fg-subtle">
                      {busy ? (
                        <span className="inline-flex items-center gap-1 text-accent-text">
                          <span className="size-1.5 animate-pulse-soft rounded-full bg-accent" />
                          <span className="sr-only">answering,</span>
                        </span>
                      ) : null}
                      <time dateTime={c.updated_at}>{formatRelative(c.updated_at)}</time>
                    </span>
                  </Link>
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={`Actions for ${conversationTitle(c)}`}
                        className="shrink-0 opacity-100 data-[state=open]:opacity-100 md:opacity-0 md:group-focus-within/item:opacity-100 md:group-hover/item:opacity-100"
                      >
                        <EllipsisIcon />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end">
                      <DropdownMenuItem onSelect={() => setRenaming(c)}>
                        <PencilIcon /> Rename
                      </DropdownMenuItem>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem tone="danger" onSelect={() => setDeleting(c)}>
                        <Trash2Icon /> Delete…
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </li>
              );
            })}
          </ul>
        )}
        {query.hasNextPage ? (
          <Button
            variant="ghost"
            size="sm"
            className="mt-2 w-full"
            disabled={query.isFetchingNextPage}
            onClick={() => void query.fetchNextPage()}
          >
            {query.isFetchingNextPage ? "Loading…" : "Older conversations"}
          </Button>
        ) : null}
      </nav>
      <RenameConversationDialog
        conversation={renaming}
        onOpenChange={(open) => !open && setRenaming(null)}
      />
      <DeleteConversationDialog
        conversation={deleting}
        onOpenChange={(open) => !open && setDeleting(null)}
        onDeleted={(id) => {
          sessions.stop(id);
          if (id === current) void navigate({ to: "/chat", search: {}, replace: true });
        }}
      />
    </div>
  );
}
