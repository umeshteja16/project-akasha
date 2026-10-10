import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FolderSyncIcon, PauseIcon, PlayIcon, RefreshCwIcon } from "lucide-react";
import { useId, useState } from "react";
import { unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { fileKeys } from "@/api/files";
import {
  type Source,
  type SourceState,
  sourceKeys,
  sourceState,
  sourcesQuery,
} from "@/api/sources";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { formatDateTime, formatRelative } from "@/lib/format";
import { sentence } from "@/lib/session";
import { AddSourceForm } from "./add-source-form";
import { SettingsSection } from "./settings-section";

const STATE: Record<
  SourceState,
  { label: string; tone: "success" | "neutral" | "danger" | "accent" }
> = {
  syncing: { label: "Syncing", tone: "accent" },
  ok: { label: "Up to date", tone: "success" },
  error: { label: "Problem", tone: "danger" },
  paused: { label: "Paused", tone: "neutral" },
};

function plural(n: number, one: string): string {
  return `${n.toLocaleString()} ${n === 1 ? one : `${one}s`}`;
}

function useSourceAction() {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({ queryKey: sourceKeys.all });
    void queryClient.invalidateQueries({ queryKey: fileKeys.all });
  };
}

function RemoveSourceDialog({ source }: { source: Source }) {
  const api = useApi();
  const refresh = useSourceAction();
  const checkbox = useId();
  const [deleteFiles, setDeleteFiles] = useState(false);
  const remove = useMutation({
    mutationFn: () =>
      unwrap(
        api.DELETE("/api/v1/sources/{id}", {
          params: { path: { id: source.id }, query: { delete_files: deleteFiles } },
        }),
      ),
    onSuccess: (removed) => {
      refresh();
      toast({
        title: removed.deleted_files
          ? `Stopped watching “${source.name}” and deleted ${plural(removed.deleted_files, "file")}`
          : `Stopped watching “${source.name}”`,
        tone: "success",
      });
    },
    onError: (error) => toast({ title: sentence(error.message), tone: "danger" }),
  });
  return (
    <Dialog onOpenChange={() => setDeleteFiles(false)}>
      <DialogTrigger asChild>
        <Button variant="secondary" size="sm" aria-label={`Remove ${source.name}`}>
          Remove
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Stop watching “{source.name}”?</DialogTitle>
          <DialogDescription>
            Nothing in the folder itself is touched. New changes there will no longer show up here.
          </DialogDescription>
        </DialogHeader>
        <label htmlFor={checkbox} className="flex cursor-pointer items-start gap-3">
          <input
            id={checkbox}
            type="checkbox"
            className="mt-0.5 size-4 shrink-0 accent-[var(--accent)]"
            checked={deleteFiles}
            onChange={(e) => setDeleteFiles(e.currentTarget.checked)}
          />
          <span className="grid gap-0.5">
            <span className="text-sm font-medium text-fg">
              Also delete the {plural(source.file_count, "file")} it imported
            </span>
            <span className="text-xs text-fg-subtle">
              Files you had uploaded yourself stay either way.
            </span>
          </span>
        </label>
        <DialogFooter>
          <DialogClose asChild>
            <Button variant="secondary">Keep watching</Button>
          </DialogClose>
          <DialogClose asChild>
            <Button variant="danger" onClick={() => remove.mutate()}>
              Stop watching
            </Button>
          </DialogClose>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function SourceRow({ source }: { source: Source }) {
  const api = useApi();
  const refresh = useSourceAction();
  const state = sourceState(source);
  const { label, tone } = STATE[state];
  const onError = (error: Error) => toast({ title: sentence(error.message), tone: "danger" });
  const scan = useMutation({
    mutationFn: () =>
      unwrap(api.POST("/api/v1/sources/{id}/scan", { params: { path: { id: source.id } } })),
    onSuccess: refresh,
    onError,
  });
  const toggle = useMutation({
    mutationFn: (enabled: boolean) =>
      unwrap(
        api.PATCH("/api/v1/sources/{id}", {
          params: { path: { id: source.id } },
          body: { enabled },
        }),
      ),
    onSuccess: refresh,
    onError,
  });
  const counts = [
    plural(source.file_count, "file"),
    source.skipped_count ? `${source.skipped_count.toLocaleString()} skipped` : null,
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <li className="grid gap-3 border-b border-border py-4 first:pt-0 last:border-b-0 last:pb-0">
      <div className="grid min-w-0 gap-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="truncate font-medium text-fg">{source.name}</span>
          <Badge tone={tone}>{label}</Badge>
        </div>
        <p className="truncate font-mono text-xs text-fg-muted" title={source.path}>
          {source.path}
        </p>
        <p className="text-xs text-fg-muted">
          {counts}
          {" · "}
          {source.last_scan_at ? (
            <>
              synced{" "}
              <time dateTime={source.last_scan_at} title={formatDateTime(source.last_scan_at)}>
                {formatRelative(source.last_scan_at)}
              </time>
            </>
          ) : (
            "not synced yet"
          )}
        </p>
        {state === "error" && source.last_error ? (
          <p className="text-xs text-danger" role="status">
            {sentence(source.last_error)}
          </p>
        ) : null}
      </div>
      <div className="flex flex-wrap gap-2">
        <Button
          variant="secondary"
          size="sm"
          disabled={!source.enabled || scan.isPending}
          onClick={() => scan.mutate()}
          aria-label={`Rescan ${source.name} now`}
        >
          <RefreshCwIcon aria-hidden />
          Rescan now
        </Button>
        <Button
          variant="secondary"
          size="sm"
          disabled={toggle.isPending}
          onClick={() => toggle.mutate(!source.enabled)}
          aria-label={`${source.enabled ? "Pause" : "Resume"} ${source.name}`}
        >
          {source.enabled ? <PauseIcon aria-hidden /> : <PlayIcon aria-hidden />}
          {source.enabled ? "Pause" : "Resume"}
        </Button>
        <RemoveSourceDialog source={source} />
      </div>
    </li>
  );
}

/** Settings → Sources: folders on the server that are imported and kept in sync. */
export function SourcesPanel() {
  const api = useApi();
  const sources = useQuery(sourcesQuery(api));
  if (sources.isPending) return <Skeleton className="mt-8 h-40" />;
  if (sources.isError) {
    return <p className="pt-8 text-sm text-danger">{sentence(sources.error.message)}</p>;
  }
  const { items, roots } = sources.data;
  return (
    <>
      <SettingsSection
        id="new-source"
        title="Watch a folder"
        description={
          <>
            Import a folder on the server, such as an Obsidian vault or a Documents folder, and keep
            it in sync: new and changed files are added within seconds, renamed ones keep their
            tags, and deleted ones go away. Markdown front-matter tags become tags.
          </>
        }
      >
        {roots.length ? (
          <AddSourceForm roots={roots} />
        ) : (
          <Card>
            <CardContent className="grid gap-2 text-sm text-fg-muted">
              <FolderSyncIcon aria-hidden className="size-5 text-fg-subtle" />
              <p>
                Watched folders are off on this server. The administrator can allow folders with{" "}
                <code className="font-mono text-fg">AKASHA_WATCH_ROOTS</code> (and mount them into
                the container).
              </p>
            </CardContent>
          </Card>
        )}
      </SettingsSection>
      <SettingsSection
        id="sources"
        title="Watched folders"
        description="Pausing stops syncing; files already imported stay. Akasha never changes the folders."
      >
        <Card>
          <CardContent>
            {items.length === 0 ? (
              <p className="text-sm text-fg-muted">No watched folders yet.</p>
            ) : (
              <ul aria-label="Watched folders">
                {items.map((s) => (
                  <SourceRow key={s.id} source={s} />
                ))}
              </ul>
            )}
          </CardContent>
        </Card>
      </SettingsSection>
    </>
  );
}
