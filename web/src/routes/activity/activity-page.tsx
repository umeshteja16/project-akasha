import { useInfiniteQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { getRouteApi, Link } from "@tanstack/react-router";
import { EraserIcon, HistoryIcon, KeyRoundIcon, LockIcon } from "lucide-react";
import { useMemo, useState } from "react";
import { type ActivityCategory, activityKeys, activityQuery } from "@/api/activity";
import { unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { EmptyState } from "@/components/common/empty-state";
import { PageHeader } from "@/components/common/page-header";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Chip } from "@/components/ui/chip";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { describe } from "@/features/activity/describe";
import { groupByDay, timeOf } from "@/features/activity/group";
import { formatDateTime } from "@/lib/format";
import { sentence } from "@/lib/session";
import { useDocumentTitle } from "@/lib/use-document-title";
import { cn } from "@/lib/utils";

const route = getRouteApi("/app/activity");

const FILTERS: ReadonlyArray<{ value?: ActivityCategory; label: string }> = [
  { label: "Everything" },
  { value: "files", label: "Files" },
  { value: "search", label: "Searches" },
  { value: "chat", label: "Questions" },
  { value: "collections", label: "Collections" },
  { value: "security", label: "Security" },
];

/** The activity timeline, grouped by day, newest first. */
export function ActivityPage() {
  useDocumentTitle("Activity");
  const api = useApi();
  const { category } = route.useSearch();
  const navigate = route.useNavigate();
  const query = useInfiniteQuery(activityQuery(api, category));
  const items = useMemo(() => query.data?.pages.flatMap((p) => p.items) ?? [], [query.data]);
  const groups = useMemo(() => groupByDay(items), [items]);
  const [clearing, setClearing] = useState(false);

  return (
    <div className="grid gap-6">
      <PageHeader
        eyebrow="History"
        title="Activity"
        description={
          <>
            What happened in your library and account, newest first. Only you can see it.{" "}
            <Link
              to="/settings"
              search={{ tab: "security" }}
              className="text-accent-text underline underline-offset-2 hover:decoration-2"
            >
              Search history and sign-ins
            </Link>
          </>
        }
        actions={
          <Button variant="secondary" onClick={() => setClearing(true)}>
            <EraserIcon /> Clear history…
          </Button>
        }
      />

      <fieldset className="-mx-4 flex min-w-0 gap-1.5 overflow-x-auto px-4 pb-0.5 [scrollbar-width:none] sm:mx-0 sm:px-0">
        <legend className="sr-only">Show</legend>
        {FILTERS.map((f) => (
          <Chip
            key={f.label}
            pressed={category === f.value}
            onClick={() =>
              void navigate({ search: f.value ? { category: f.value } : {}, replace: true })
            }
          >
            {f.value === "security" ? <LockIcon /> : null}
            {f.label}
          </Chip>
        ))}
      </fieldset>

      {query.isPending ? (
        <div className="grid gap-4" role="status" aria-busy="true" aria-label="Loading activity">
          {[0, 1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-10" />
          ))}
        </div>
      ) : query.isError ? (
        <p
          role="alert"
          className="rounded-lg border border-danger/30 bg-danger-soft px-4 py-3 text-sm text-danger"
        >
          Couldn't load your activity. Try again in a moment.
        </p>
      ) : items.length === 0 ? (
        <EmptyState icon={HistoryIcon} title="Nothing here yet">
          <p>
            {category
              ? "Nothing of this kind has happened yet, or it was cleared."
              : "Uploads, searches, questions and sign-ins will appear here as they happen."}
          </p>
        </EmptyState>
      ) : (
        <div className="grid gap-8">
          {groups.map((group) => (
            <section key={group.key} aria-labelledby={`day-${group.key}`} className="grid gap-3">
              <h2
                id={`day-${group.key}`}
                className="display flex items-center gap-3 text-xl text-fg"
              >
                {group.label}
                <span className="h-px flex-1 bg-border" aria-hidden />
              </h2>
              <ol className="grid">
                {group.items.map((item) => {
                  const d = describe(item);
                  const Icon = d.icon;
                  return (
                    <li key={item.id} className="group relative flex gap-3 py-2.5 pl-1">
                      <span
                        aria-hidden
                        className="absolute top-0 bottom-0 left-[1.0625rem] w-px bg-border group-first:top-4 group-last:bottom-auto group-last:h-4"
                      />
                      <span
                        aria-hidden
                        className={cn(
                          "relative z-10 grid size-8 shrink-0 place-items-center rounded-full border bg-surface [&_svg]:size-4",
                          d.tone === "alert"
                            ? "border-danger/40 text-danger"
                            : "border-border text-fg-subtle",
                        )}
                      >
                        <Icon strokeWidth={1.75} />
                      </span>
                      <div className="grid min-w-0 flex-1 gap-0.5 pt-1">
                        <p
                          className={cn(
                            "text-sm break-words text-fg-muted",
                            d.tone === "alert" && "text-danger",
                          )}
                        >
                          {d.text}
                        </p>
                        {d.meta || item.via === "token" ? (
                          <p className="flex flex-wrap items-center gap-2 text-xs text-fg-subtle">
                            {item.via === "token" ? (
                              <Badge tone="neutral">
                                <KeyRoundIcon className="size-3" aria-hidden /> API token
                              </Badge>
                            ) : null}
                            {d.meta ? <span className="break-words">{d.meta}</span> : null}
                          </p>
                        ) : null}
                      </div>
                      <time
                        dateTime={item.created_at}
                        title={formatDateTime(item.created_at)}
                        className="shrink-0 pt-1.5 font-mono text-2xs text-fg-subtle"
                      >
                        {timeOf(item.created_at)}
                      </time>
                    </li>
                  );
                })}
              </ol>
            </section>
          ))}
          {query.hasNextPage ? (
            <div className="flex justify-center">
              <Button
                variant="secondary"
                onClick={() => void query.fetchNextPage()}
                disabled={query.isFetchingNextPage}
              >
                {query.isFetchingNextPage ? "Loading…" : "Show older"}
              </Button>
            </div>
          ) : null}
        </div>
      )}

      <ClearHistoryDialog open={clearing} onOpenChange={setClearing} />
    </div>
  );
}

function ClearHistoryDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const api = useApi();
  const queryClient = useQueryClient();
  const clear = useMutation({
    mutationFn: () => unwrap(api.DELETE("/api/v1/activity", { params: { query: {} } })),
    onSuccess: (res) => {
      onOpenChange(false);
      void queryClient.invalidateQueries({ queryKey: activityKeys.all });
      toast({
        title: res.deleted === 1 ? "Cleared 1 entry" : `Cleared ${res.deleted} entries`,
        tone: "success",
      });
    },
    onError: (e) => toast({ title: sentence(e.message), tone: "danger" }),
  });
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Clear your history?</DialogTitle>
          <DialogDescription>
            Removes uploads, searches, questions and collection changes from this timeline. Your
            files are not touched. The security log (sign-ins, passwords, tokens) is kept.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="secondary" onClick={() => onOpenChange(false)}>
            Keep it
          </Button>
          <Button variant="danger" onClick={() => clear.mutate()} disabled={clear.isPending}>
            {clear.isPending ? "Clearing…" : "Clear history"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
