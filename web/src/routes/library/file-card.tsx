import { Link } from "@tanstack/react-router";
import { PinIcon, RotateCwIcon, SparklesIcon } from "lucide-react";
import type { FileItem } from "@/api/files";
import { Button } from "@/components/ui/button";
import { FileThumb } from "@/features/files/file-thumb";
import { kindOf } from "@/features/files/kind";
import { StatusBadge } from "@/features/files/status-badge";
import { formatBytes, formatRelative } from "@/lib/format";
import { cn } from "@/lib/utils";

export interface FileCardProps {
  file: FileItem;
  index: number;
  /** The roving-focus item (tab stop). */
  active: boolean;
  selected: boolean;
  selecting: boolean;
  onSelect: (id: string, selected: boolean) => void;
  onTogglePin: (file: FileItem) => void;
  onRetry: (file: FileItem) => void;
  onFocusItem: (index: number) => void;
}

function TagList({ file, max }: { file: FileItem; max: number }) {
  const tags = [
    ...file.tags.map((t) => ({ tag: t, auto: false })),
    ...file.auto_tags.map((t) => ({ tag: t, auto: true })),
  ];
  if (tags.length === 0) return null;
  const shown = tags.slice(0, max);
  return (
    <ul className="flex min-w-0 flex-wrap gap-1" aria-label="Tags">
      {shown.map(({ tag, auto }) => (
        <li
          key={`${auto ? "a" : "u"}-${tag}`}
          className={cn(
            "inline-flex max-w-full items-center gap-1 truncate rounded-sm px-1.5 py-0.5 font-mono text-2xs",
            auto
              ? "border border-dashed border-border-strong/50 text-fg-subtle"
              : "bg-surface-2 text-fg-muted",
          )}
          title={auto ? `${tag} (suggested)` : tag}
        >
          {auto ? <SparklesIcon className="size-2.5 shrink-0" aria-label="suggested" /> : null}
          <span className="truncate">{tag}</span>
        </li>
      ))}
      {tags.length > max ? (
        <li className="px-1 py-0.5 font-mono text-2xs text-fg-subtle">+{tags.length - max}</li>
      ) : null}
    </ul>
  );
}

function SelectBox({
  file,
  selected,
  onSelect,
  className,
}: Pick<FileCardProps, "file" | "selected" | "onSelect"> & { className?: string }) {
  return (
    <label
      className={cn(
        "grid size-7 cursor-pointer place-items-center rounded-md bg-surface/90 shadow-xs backdrop-blur-sm",
        className,
      )}
    >
      <input
        type="checkbox"
        checked={selected}
        onChange={(e) => onSelect(file.id, e.currentTarget.checked)}
        aria-label={`Select ${file.name}`}
        className="size-4 cursor-pointer accent-[var(--accent)]"
      />
    </label>
  );
}

function PinButton({
  file,
  onTogglePin,
  className,
}: Pick<FileCardProps, "file" | "onTogglePin"> & { className?: string }) {
  return (
    <button
      type="button"
      aria-pressed={file.is_pinned}
      aria-label={file.is_pinned ? `Unpin ${file.name}` : `Pin ${file.name}`}
      onClick={() => onTogglePin(file)}
      className={cn(
        "grid size-7 place-items-center rounded-md transition-colors",
        file.is_pinned ? "text-accent-text" : "text-fg-subtle hover:text-fg",
        className,
      )}
    >
      <PinIcon className={cn("size-3.5", file.is_pinned && "fill-current")} />
    </button>
  );
}

function meta(file: FileItem): string {
  return `${kindOf(file.mime_type).label} · ${formatBytes(file.size_bytes)} · ${formatRelative(file.created_at)}`;
}

/** A grid card: thumbnail, name, facts, tags. */
export function FileTile(props: FileCardProps) {
  const { file, index, active, selected, selecting, onFocusItem, onRetry } = props;
  return (
    <article
      data-selected={selected || undefined}
      className={cn(
        "group relative flex h-full flex-col overflow-hidden rounded-lg border border-border bg-surface shadow-xs",
        "transition-[border-color,box-shadow] hover:border-border-strong/60 hover:shadow-sm",
        "has-[a:focus-visible]:border-accent has-[a:focus-visible]:ring-3 has-[a:focus-visible]:ring-accent/20",
        selected && "border-accent ring-3 ring-accent/20",
      )}
    >
      <Link
        to="/files/$fileId"
        params={{ fileId: file.id }}
        data-nav-item={index}
        tabIndex={active ? 0 : -1}
        onFocus={() => onFocusItem(index)}
        className="flex flex-1 flex-col outline-none"
      >
        <FileThumb
          id={file.id}
          mime={file.mime_type}
          status={file.status}
          className="border-b border-border"
        />
        <div className="grid flex-1 content-start gap-1.5 p-3">
          <h3 className="line-clamp-2 text-sm font-medium break-words text-fg" title={file.name}>
            {file.name}
          </h3>
          <p className="truncate text-xs text-fg-subtle">{meta(file)}</p>
          <TagList file={file} max={3} />
        </div>
      </Link>
      {file.status !== "ready" ? (
        <div className="flex items-center gap-2 px-3 pb-3">
          <StatusBadge status={file.status} />
          {file.status === "failed" ? (
            <Button variant="link" size="sm" className="h-6 text-xs" onClick={() => onRetry(file)}>
              <RotateCwIcon className="size-3" /> Retry
            </Button>
          ) : null}
        </div>
      ) : null}
      <SelectBox
        {...props}
        className={cn(
          "absolute top-2 left-2 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100",
          (selected || selecting) && "opacity-100",
        )}
      />
      <PinButton
        {...props}
        className={cn(
          "absolute top-2 right-2 bg-surface/90 opacity-0 shadow-xs backdrop-blur-sm transition-opacity",
          "group-hover:opacity-100 focus-visible:opacity-100",
          file.is_pinned && "opacity-100",
        )}
      />
    </article>
  );
}

/** A list row: icon, name and tags, facts, status, pin. */
export function FileRow(props: FileCardProps) {
  const { file, index, active, selected, onFocusItem, onRetry } = props;
  const kind = kindOf(file.mime_type);
  return (
    <div
      data-selected={selected || undefined}
      className={cn(
        "group grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 px-3 py-2.5 transition-colors sm:grid-cols-[auto_minmax(0,1fr)_7rem_5rem_7.5rem_auto] sm:px-4",
        "hover:bg-surface-2/60 has-[a:focus-visible]:bg-surface-2",
        selected && "bg-accent-soft/50 hover:bg-accent-soft/60",
      )}
    >
      <div className="flex items-center gap-2">
        <SelectBox {...props} className="size-6 bg-transparent shadow-none" />
        <FileThumb id={file.id} mime={file.mime_type} status={file.status} variant="icon" />
      </div>
      <div className="min-w-0">
        <Link
          to="/files/$fileId"
          params={{ fileId: file.id }}
          data-nav-item={index}
          tabIndex={active ? 0 : -1}
          onFocus={() => onFocusItem(index)}
          className="block truncate rounded-xs text-sm font-medium text-fg outline-none hover:underline focus-visible:underline"
          title={file.name}
        >
          {file.name}
        </Link>
        <div className="mt-0.5 flex min-w-0 items-center gap-2">
          <span className="shrink-0 text-xs text-fg-subtle sm:hidden">
            {formatBytes(file.size_bytes)} · {formatRelative(file.created_at)}
          </span>
          <TagList file={file} max={4} />
        </div>
        {file.status === "failed" && file.error ? (
          <p className="mt-1 truncate text-xs text-danger">{file.error}</p>
        ) : null}
      </div>
      <span className="hidden truncate text-xs text-fg-muted sm:block">{kind.label}</span>
      <span className="hidden text-right font-mono text-xs text-fg-muted sm:block">
        {formatBytes(file.size_bytes)}
      </span>
      <span className="hidden text-xs text-fg-muted sm:block">
        {formatRelative(file.created_at)}
      </span>
      <div className="flex items-center justify-end gap-1">
        <StatusBadge status={file.status} />
        {file.status === "failed" ? (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Retry ${file.name}`}
            onClick={() => onRetry(file)}
          >
            <RotateCwIcon />
          </Button>
        ) : null}
        <PinButton
          {...props}
          className={cn(
            !file.is_pinned && "opacity-0 group-hover:opacity-100 focus-visible:opacity-100",
          )}
        />
      </div>
    </div>
  );
}
