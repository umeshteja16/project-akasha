import { Link } from "@tanstack/react-router";
import {
  ChevronDownIcon,
  CircleAlertIcon,
  CircleCheckIcon,
  CopyCheckIcon,
  RotateCwIcon,
  XIcon,
} from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useUploader, useUploads } from "./upload-context";
import { isActive, overallProgress, type UploadItem } from "./upload-queue";

function summary(items: readonly UploadItem[]): string {
  const active = items.filter(isActive).length;
  if (active > 0) return active === 1 ? "Uploading 1 file" : `Uploading ${active} files`;
  const failed = items.filter((i) => i.state === "failed").length;
  if (failed > 0)
    return failed === 1 ? "1 upload needs attention" : `${failed} uploads need attention`;
  return items.length === 1 ? "Upload finished" : `${items.length} uploads finished`;
}

/** Floating list of uploads with per-file progress, cancel and retry. */
export function UploadPanel() {
  const items = useUploads();
  const { queue } = useUploader();
  const [collapsed, setCollapsed] = useState(false);
  if (items.length === 0) return null;

  const active = items.some(isActive);
  const progress = overallProgress(items);
  return (
    <section
      aria-label="Uploads"
      className={cn(
        "fixed inset-x-3 bottom-[calc(env(safe-area-inset-bottom)+4.75rem)] z-40 animate-rise-in",
        "overflow-hidden rounded-lg border border-border bg-surface shadow-lg",
        "md:inset-x-auto md:bottom-6 md:left-[calc(var(--sidebar-width)+1.5rem)] md:w-[24rem]",
      )}
    >
      <header className="flex items-center gap-2 py-2 pr-2 pl-4">
        <p className="flex-1 text-sm font-medium text-fg" aria-live="polite">
          {summary(items)}
          {active ? (
            <span className="ml-2 font-mono text-xs text-fg-subtle">
              {Math.round(progress * 100)}%
            </span>
          ) : null}
        </p>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={collapsed ? "Show uploads" : "Hide uploads"}
          aria-expanded={!collapsed}
          onClick={() => setCollapsed((c) => !c)}
        >
          <ChevronDownIcon className={cn("transition-transform", collapsed && "rotate-180")} />
        </Button>
        {active ? null : (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Close uploads"
            onClick={() => queue.clearFinished()}
          >
            <XIcon />
          </Button>
        )}
      </header>
      {active ? (
        <Progress value={progress} label="All uploads" className="h-0.5 rounded-none" />
      ) : null}
      {collapsed ? null : (
        <ul className="max-h-[min(50vh,20rem)] divide-y divide-border overflow-y-auto border-t border-border">
          {items.map((item) => (
            <UploadRow key={item.id} item={item} />
          ))}
        </ul>
      )}
    </section>
  );
}

function UploadRow({ item }: { item: UploadItem }) {
  const { queue } = useUploader();
  const fraction = item.size > 0 ? item.loaded / item.size : 0;
  const finished = !isActive(item);
  return (
    <li className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-x-3 gap-y-1.5 px-4 py-3">
      <div className="min-w-0">
        <p className="truncate text-sm text-fg" title={item.name}>
          {item.name}
        </p>
        <RowStatus item={item} fraction={fraction} />
      </div>
      <div className="flex items-center gap-1">
        {(item.state === "failed" && item.problem?.retryable) || item.state === "cancelled" ? (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Retry ${item.name}`}
            onClick={() => queue.retry(item.id)}
          >
            <RotateCwIcon />
          </Button>
        ) : null}
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={finished ? `Dismiss ${item.name}` : `Cancel upload of ${item.name}`}
          onClick={() => (finished ? queue.dismiss(item.id) : queue.cancel(item.id))}
        >
          <XIcon />
        </Button>
      </div>
      {item.state === "uploading" ? (
        <Progress value={fraction} label={`Uploading ${item.name}`} className="col-span-2" />
      ) : null}
    </li>
  );
}

function RowStatus({ item, fraction }: { item: UploadItem; fraction: number }) {
  const line = "mt-0.5 flex items-center gap-1.5 text-xs";
  switch (item.state) {
    case "queued":
      return <p className={cn(line, "text-fg-subtle")}>Waiting · {formatBytes(item.size)}</p>;
    case "uploading":
      return (
        <p className={cn(line, "font-mono text-fg-subtle")}>
          {Math.round(fraction * 100)}% · {formatBytes(item.loaded)} of {formatBytes(item.size)}
        </p>
      );
    case "done":
      return (
        <p className={cn(line, "text-fg-muted")}>
          <CircleCheckIcon className="size-3.5 text-accent-text" aria-hidden />
          Added, now reading it.
          {item.file ? <OpenLink id={item.file.id}>Open</OpenLink> : null}
        </p>
      );
    case "duplicate":
      return (
        <p className={cn(line, "text-fg-muted")}>
          <CopyCheckIcon className="size-3.5 text-fg-subtle" aria-hidden />
          Already in your library.
          {item.file ? <OpenLink id={item.file.id}>Show it</OpenLink> : null}
        </p>
      );
    case "failed":
      return (
        <p className={cn(line, "items-start text-danger")}>
          <CircleAlertIcon className="mt-0.5 size-3.5 shrink-0" aria-hidden />
          <span>
            <span className="font-medium">{item.problem?.title}.</span> {item.problem?.message}
          </span>
        </p>
      );
    case "cancelled":
      return <p className={cn(line, "text-fg-subtle")}>Cancelled</p>;
  }
}

function OpenLink({ id, children }: { id: string; children: string }) {
  return (
    <Link
      to="/files/$fileId"
      params={{ fileId: id }}
      className="font-medium text-accent-text underline-offset-4 hover:underline"
    >
      {children}
    </Link>
  );
}
