import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { PlusIcon } from "lucide-react";
import { lazy, Suspense, useState } from "react";
import { collectionsQuery } from "@/api/collections";
import { useApi } from "@/api/context";
import { COLORS } from "@/features/collections/look";
import { cn } from "@/lib/utils";

// Loaded on first use: the shell stays light.
const CollectionFormDialog = lazy(() =>
  import("@/features/collections/collection-form-dialog").then((m) => ({
    default: m.CollectionFormDialog,
  })),
);

export const navLink =
  "group relative flex h-9 items-center gap-3 rounded-md px-3 text-sm text-fg-muted transition-colors hover:bg-surface-2 hover:text-fg data-[status=active]:bg-surface data-[status=active]:font-medium data-[status=active]:text-fg data-[status=active]:shadow-xs";

export function ActiveMarker() {
  return (
    <span
      aria-hidden
      className="absolute top-2 bottom-2 left-0 w-0.5 rounded-full bg-accent opacity-0 transition-opacity group-data-[status=active]:opacity-100"
    />
  );
}

/** Shown in the sidebar; the rest are on the Collections page. */
const SHOWN = 8;

/** The sidebar's list of collections, with "New collection". */
export function SidebarCollections() {
  const api = useApi();
  const query = useQuery(collectionsQuery(api));
  const [creating, setCreating] = useState(false);
  const items = query.data?.items ?? [];
  return (
    <div className="mt-6 grid gap-0.5">
      <div className="flex items-center justify-between pr-1 pb-1 pl-3">
        <p className="eyebrow">Collections</p>
        <button
          type="button"
          onClick={() => setCreating(true)}
          aria-label="New collection"
          className="grid size-6 place-items-center rounded-sm text-fg-subtle transition-colors hover:bg-surface-2 hover:text-fg"
        >
          <PlusIcon className="size-3.5" aria-hidden />
        </button>
      </div>
      {query.isSuccess && items.length === 0 ? (
        <button
          type="button"
          onClick={() => setCreating(true)}
          className="px-3 py-1.5 text-left text-xs text-fg-subtle hover:text-fg"
        >
          Group related files into a collection.
        </button>
      ) : null}
      {items.slice(0, SHOWN).map((c) => (
        <Link
          key={c.id}
          to="/collections/$collectionId"
          params={{ collectionId: c.id }}
          className={cn(navLink, "h-8")}
        >
          <ActiveMarker />
          <span className={cn("size-2 shrink-0 rounded-full", COLORS[c.color]?.dot)} aria-hidden />
          <span className="min-w-0 flex-1 truncate">{c.name}</span>
          <span className="font-mono text-2xs text-fg-subtle">{c.file_count}</span>
        </Link>
      ))}
      {items.length > SHOWN ? (
        <Link to="/collections" className="px-3 py-1.5 text-xs text-fg-subtle hover:text-fg">
          All {items.length} collections
        </Link>
      ) : null}
      {creating ? (
        <Suspense fallback={null}>
          <CollectionFormDialog open={creating} onOpenChange={setCreating} />
        </Suspense>
      ) : null}
    </div>
  );
}
