import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useApi } from "@/api/context";
import { type FileItem, similarQuery } from "@/api/files";
import { Skeleton } from "@/components/ui/skeleton";
import { FileThumb } from "@/features/files/file-thumb";
import { kindOf } from "@/features/files/kind";

/** Files whose contents are closest to this one's. */
export function SimilarFiles({ file }: { file: FileItem }) {
  const api = useApi();
  const ready = file.status === "ready";
  const query = useQuery(similarQuery(api, file.id, ready));

  if (!ready) {
    return <p className="text-xs text-fg-subtle">Appears once the file has been read.</p>;
  }
  if (query.isPending) {
    return (
      <div className="grid gap-2" aria-busy="true">
        <Skeleton className="h-10" />
        <Skeleton className="h-10" />
      </div>
    );
  }
  const items = query.data?.items ?? [];
  if (items.length === 0) {
    return (
      <p className="text-xs text-fg-subtle">
        Nothing similar yet. Related files show up here as your library grows.
      </p>
    );
  }
  return (
    <ul className="-mx-2 grid">
      {items.map(({ file: other, similarity }) => (
        <li key={other.id}>
          <Link
            to="/files/$fileId"
            params={{ fileId: other.id }}
            className="flex items-center gap-3 rounded-md px-2 py-1.5 hover:bg-surface-2"
          >
            <FileThumb
              id={other.id}
              mime={other.mime_type}
              status={other.status}
              variant="icon"
              className="size-8"
            />
            <span className="min-w-0 flex-1">
              <span className="block truncate text-sm text-fg">{other.name}</span>
              <span className="block text-xs text-fg-subtle">{kindOf(other.mime_type).label}</span>
            </span>
            <span className="font-mono text-2xs text-fg-subtle" title="How close the contents are">
              {Math.round(similarity * 100)}%<span className="sr-only"> similar</span>
            </span>
          </Link>
        </li>
      ))}
    </ul>
  );
}
