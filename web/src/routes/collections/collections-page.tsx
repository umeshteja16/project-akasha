import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { FolderPlusIcon, LibraryBigIcon, PlusIcon } from "lucide-react";
import { useState } from "react";
import { collectionsQuery } from "@/api/collections";
import { useApi } from "@/api/context";
import { EmptyState } from "@/components/common/empty-state";
import { PageHeader } from "@/components/common/page-header";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { CollectionFormDialog } from "@/features/collections/collection-form-dialog";
import { CollectionMark } from "@/features/collections/look";
import { formatRelative } from "@/lib/format";
import { useDocumentTitle } from "@/lib/use-document-title";

/** All collections as cards, and "New collection". */
export function CollectionsPage() {
  useDocumentTitle("Collections");
  const api = useApi();
  const navigate = useNavigate();
  const query = useQuery(collectionsQuery(api));
  const [creating, setCreating] = useState(false);
  const items = query.data?.items ?? [];

  return (
    <div className="grid gap-6">
      <PageHeader
        eyebrow="Your archive"
        title="Collections"
        description="Named groups of files, like a project, a trip or a tax year. Search or ask within one; a file can be in several."
        actions={
          <Button onClick={() => setCreating(true)}>
            <PlusIcon /> New collection
          </Button>
        }
      />
      {query.isPending ? (
        <div
          className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3"
          role="status"
          aria-busy="true"
          aria-label="Loading collections"
        >
          {[0, 1, 2].map((i) => (
            <Skeleton key={i} className="h-32" />
          ))}
        </div>
      ) : query.isError ? (
        <p
          role="alert"
          className="rounded-lg border border-danger/30 bg-danger-soft px-4 py-3 text-sm text-danger"
        >
          Couldn't load your collections. Try again in a moment.
        </p>
      ) : items.length === 0 ? (
        <EmptyState
          icon={FolderPlusIcon}
          title="No collections yet"
          actions={
            <>
              <Button onClick={() => setCreating(true)}>
                <PlusIcon /> New collection
              </Button>
              <Button variant="secondary" asChild>
                <Link to="/library">
                  <LibraryBigIcon /> Go to the library
                </Link>
              </Button>
            </>
          }
        >
          <p>
            Group files that belong together, then search or ask within just those. In the library,
            select files and choose “Add to collection”.
          </p>
        </EmptyState>
      ) : (
        <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {items.map((c) => (
            <li key={c.id}>
              <Link
                to="/collections/$collectionId"
                params={{ collectionId: c.id }}
                className="group flex h-full flex-col gap-3 rounded-lg border border-border bg-surface p-4 shadow-xs transition-colors hover:border-border-strong"
              >
                <div className="flex items-start gap-3">
                  <CollectionMark color={c.color} icon={c.icon} />
                  <div className="grid min-w-0 flex-1 gap-0.5">
                    <h2 className="display truncate text-xl text-fg group-hover:underline">
                      {c.name}
                    </h2>
                    <p className="text-xs text-fg-subtle">
                      {c.file_count === 1 ? "1 file" : `${c.file_count} files`} · updated{" "}
                      {formatRelative(c.updated_at)}
                    </p>
                  </div>
                </div>
                {c.description ? (
                  <p className="line-clamp-2 text-sm text-fg-muted">{c.description}</p>
                ) : null}
              </Link>
            </li>
          ))}
        </ul>
      )}
      <CollectionFormDialog
        open={creating}
        onOpenChange={setCreating}
        onSaved={(c) =>
          void navigate({ to: "/collections/$collectionId", params: { collectionId: c.id } })
        }
      />
    </div>
  );
}
