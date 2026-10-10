import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { getRouteApi, Link, useNavigate } from "@tanstack/react-router";
import {
  ArrowLeftIcon,
  EllipsisIcon,
  FolderMinusIcon,
  FolderSearchIcon,
  MessageSquareQuoteIcon,
  PencilIcon,
  PlusIcon,
  SearchIcon,
  Trash2Icon,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { isApiError } from "@/api/client";
import { type Collection, collectionQuery } from "@/api/collections";
import { useApi } from "@/api/context";
import { type FileItem, filesQuery } from "@/api/files";
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
import { toast } from "@/components/ui/toast";
import { CollectionFormDialog } from "@/features/collections/collection-form-dialog";
import { CollectionMark } from "@/features/collections/look";
import { useCollectionFiles } from "@/features/collections/mutations";
import { ConfirmDeleteDialog } from "@/features/files/confirm-delete-dialog";
import { useReindex, useTogglePin } from "@/features/files/mutations";
import { formatRelative } from "@/lib/format";
import { sentence } from "@/lib/session";
import { useDocumentTitle } from "@/lib/use-document-title";
import { FileCollection } from "@/routes/library/file-collection";
import { LibraryLoading } from "@/routes/library/library-empty";
import { SelectionBar } from "@/routes/library/selection-bar";
import { useLibraryPrefs } from "@/routes/library/use-library-prefs";
import { AddFilesDialog } from "./add-files-dialog";
import { DeleteCollectionDialog } from "./delete-collection-dialog";

const route = getRouteApi("/app/collections/$collectionId");

export function CollectionPage() {
  const { collectionId } = route.useParams();
  const api = useApi();
  const query = useQuery(collectionQuery(api, collectionId));
  useDocumentTitle(query.data?.name ?? (query.isError ? "Collection not found" : "Collection"));

  if (query.isPending) {
    return (
      <div className="grid gap-6" role="status" aria-busy="true" aria-label="Loading">
        <Skeleton className="h-4 w-24" />
        <Skeleton className="h-24 w-full" />
      </div>
    );
  }
  if (query.isError) {
    const missing = isApiError(query.error) && [400, 404].includes(query.error.status);
    return (
      <EmptyState
        icon={FolderSearchIcon}
        title={missing ? "This collection doesn't exist" : "Couldn't open this collection"}
        actions={
          <Button variant="secondary" asChild>
            <Link to="/collections">All collections</Link>
          </Button>
        }
      >
        <p>
          {missing ? "It may have been deleted, or the link is wrong." : "Try again in a moment."}
        </p>
      </EmptyState>
    );
  }
  return <CollectionView collection={query.data} />;
}

function CollectionView({ collection }: { collection: Collection }) {
  const api = useApi();
  const navigate = useNavigate();
  const [prefs] = useLibraryPrefs();
  const sort = prefs.sort === "opened" ? "newest" : prefs.sort;
  const files = useInfiniteQuery(filesQuery(api, { collection: collection.id }, sort));
  const items = useMemo(() => files.data?.pages.flatMap((p) => p.items) ?? [], [files.data]);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState(false);
  const [deletingCollection, setDeletingCollection] = useState(false);
  const [deletingFiles, setDeletingFiles] = useState<string[] | null>(null);
  const change = useCollectionFiles();
  const togglePin = useTogglePin();
  const reindex = useReindex();

  useEffect(() => {
    setSelected((old) => {
      const ids = new Set(items.map((f) => f.id));
      const kept = [...old].filter((id) => ids.has(id));
      return kept.length === old.size ? old : new Set(kept);
    });
  }, [items]);

  const select = (id: string, on: boolean) =>
    setSelected((old) => {
      const next = new Set(old);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });

  const removeSelected = () => {
    const ids = [...selected].slice(0, 100);
    change.mutate(
      { id: collection.id, fileIds: ids, remove: true },
      {
        onSuccess: (res) => {
          setSelected(new Set());
          const n = res.file_ids.length;
          toast({
            title: `Removed ${n === 1 ? "1 file" : `${n} files`} from “${collection.name}”`,
            description: "The files are still in your library.",
            tone: "success",
          });
        },
        onError: (e) => toast({ title: sentence(e.message), tone: "danger" }),
      },
    );
  };

  const retry = (file: FileItem) => reindex.mutate(file.id);
  const count = collection.file_count;

  return (
    <div className="grid gap-6">
      <Link
        to="/collections"
        className="inline-flex w-fit items-center gap-1.5 text-xs font-medium text-fg-muted hover:text-fg"
      >
        <ArrowLeftIcon className="size-3.5" aria-hidden /> Collections
      </Link>

      <header className="flex flex-col gap-5 border-b border-border pb-6 lg:flex-row lg:items-end lg:justify-between">
        <div className="flex min-w-0 items-start gap-4">
          <CollectionMark color={collection.color} icon={collection.icon} size="lg" />
          <div className="grid min-w-0 gap-1.5">
            <p className="eyebrow">
              Collection · {count === 1 ? "1 file" : `${count} files`} · updated{" "}
              {formatRelative(collection.updated_at)}
            </p>
            <h1 className="display text-3xl break-words text-fg sm:text-4xl">{collection.name}</h1>
            {collection.description ? (
              <p className="max-w-[var(--reading-max)] text-sm whitespace-pre-line text-fg-muted">
                {collection.description}
              </p>
            ) : null}
          </div>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-2">
          <Button onClick={() => setAdding(true)}>
            <PlusIcon /> Add<span className="hidden sm:inline"> files</span>
          </Button>
          <Button variant="secondary" asChild>
            <Link to="/search" search={{ collection: collection.id }}>
              <SearchIcon /> Search<span className="hidden sm:inline"> in it</span>
            </Link>
          </Button>
          <Button variant="secondary" asChild>
            <Link to="/chat" search={{ collection: collection.id }}>
              <MessageSquareQuoteIcon /> Ask
            </Link>
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="secondary" size="icon" aria-label="More actions">
                <EllipsisIcon />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={() => setEditing(true)}>
                <PencilIcon /> Edit…
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem tone="danger" onSelect={() => setDeletingCollection(true)}>
                <Trash2Icon /> Delete collection…
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </header>

      {selected.size > 0 ? (
        <SelectionBar
          count={selected.size}
          total={items.length}
          onSelectAll={() => setSelected(new Set(items.map((f) => f.id)))}
          onClear={() => setSelected(new Set())}
          onDelete={() => setDeletingFiles([...selected])}
          onAsk={() =>
            void navigate({
              to: "/chat",
              search: { files: [...selected].slice(0, 100).join(",") },
            })
          }
          actions={
            <Button
              variant="secondary"
              size="sm"
              onClick={removeSelected}
              disabled={change.isPending || selected.size > 100}
            >
              <FolderMinusIcon />
              <span className="hidden sm:inline">Remove from collection</span>
              <span className="sm:hidden">Remove</span>
            </Button>
          }
        />
      ) : null}

      {files.isPending ? (
        <LibraryLoading view={prefs.view} />
      ) : files.isError ? (
        <p
          role="alert"
          className="rounded-lg border border-danger/30 bg-danger-soft px-4 py-3 text-sm text-danger"
        >
          Couldn't load this collection's files. Try again in a moment.
        </p>
      ) : items.length === 0 ? (
        <EmptyState
          icon={FolderSearchIcon}
          title="Nothing in here yet"
          actions={
            <Button onClick={() => setAdding(true)}>
              <PlusIcon /> Add files
            </Button>
          }
        >
          <p>
            Add files from your library here, or select files in the library and choose “Add to
            collection”. A file can be in several collections.
          </p>
        </EmptyState>
      ) : (
        <>
          <FileCollection
            files={items}
            view={prefs.view}
            selected={selected}
            selecting={selected.size > 0}
            onSelect={select}
            onTogglePin={togglePin}
            onRetry={retry}
            onDeleteRequest={setDeletingFiles}
            onClearSelection={() => setSelected(new Set())}
          />
          {files.hasNextPage ? (
            <div className="flex justify-center py-4">
              <Button
                variant="secondary"
                onClick={() => void files.fetchNextPage()}
                disabled={files.isFetchingNextPage}
              >
                {files.isFetchingNextPage ? "Loading…" : "Load more"}
              </Button>
            </div>
          ) : null}
        </>
      )}

      <AddFilesDialog collection={collection} open={adding} onOpenChange={setAdding} />
      <CollectionFormDialog open={editing} onOpenChange={setEditing} collection={collection} />
      <DeleteCollectionDialog
        collection={collection}
        open={deletingCollection}
        onOpenChange={setDeletingCollection}
        onDeleted={() => void navigate({ to: "/collections", replace: true })}
      />
      <ConfirmDeleteDialog
        open={deletingFiles !== null}
        onOpenChange={(open) => {
          if (!open) setDeletingFiles(null);
        }}
        ids={deletingFiles ?? []}
        name={
          deletingFiles?.length === 1
            ? items.find((f) => f.id === deletingFiles[0])?.name
            : undefined
        }
        onDeleted={() => setSelected(new Set())}
      />
    </div>
  );
}
