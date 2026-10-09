import { useInfiniteQuery } from "@tanstack/react-query";
import { getRouteApi, useNavigate } from "@tanstack/react-router";
import { SearchIcon, UploadIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { isApiError } from "@/api/client";
import { useApi } from "@/api/context";
import { type FileItem, filesQuery } from "@/api/files";
import { PageHeader } from "@/components/common/page-header";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Kbd } from "@/components/ui/kbd";
import { toast } from "@/components/ui/toast";
import { Tooltip } from "@/components/ui/tooltip";
import { ConfirmDeleteDialog } from "@/features/files/confirm-delete-dialog";
import { useReindex, useTogglePin } from "@/features/files/mutations";
import { useUploader } from "@/features/upload/upload-context";
import { FileCollection } from "./file-collection";
import { LibraryEmpty, LibraryLoading, NoMatches } from "./library-empty";
import { hasFilters, toFilters } from "./library-search";
import { LibraryToolbar } from "./library-toolbar";
import { SelectionBar } from "./selection-bar";
import { useLibraryPrefs } from "./use-library-prefs";

const route = getRouteApi("/app/library");

export function LibraryPage() {
  const api = useApi();
  const search = route.useSearch();
  const navigate = useNavigate();
  const [prefs, setPrefs] = useLibraryPrefs();
  const { pick } = useUploader();
  const filters = useMemo(() => toFilters(search), [search]);
  const query = useInfiniteQuery(filesQuery(api, filters, prefs.sort));
  const files = useMemo(() => query.data?.pages.flatMap((p) => p.items) ?? [], [query.data]);

  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [deleting, setDeleting] = useState<string[] | null>(null);
  const togglePin = useTogglePin();
  const reindex = useReindex();

  // Forget selected files that left the list (deleted, filtered out).
  useEffect(() => {
    setSelected((old) => {
      const ids = new Set(files.map((f) => f.id));
      const kept = [...old].filter((id) => ids.has(id));
      return kept.length === old.size ? old : new Set(kept);
    });
  }, [files]);

  const select = (id: string, on: boolean) =>
    setSelected((old) => {
      const next = new Set(old);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });

  const retry = (file: FileItem) =>
    reindex.mutate(file.id, {
      onSuccess: () => toast({ title: `Reading “${file.name}” again`, tone: "success" }),
      onError: (e) =>
        toast({
          title: "Couldn't retry",
          description: isApiError(e) ? e.message : undefined,
          tone: "danger",
        }),
    });

  const filtered = hasFilters(search);
  const deletingName =
    deleting?.length === 1 ? files.find((f) => f.id === deleting[0])?.name : undefined;

  return (
    <div className="grid gap-6">
      <PageHeader
        eyebrow="Your archive"
        title="Library"
        description="Documents, scans, images and notes you've kept. Akasha reads each one so you can find it by what it says."
        actions={
          <>
            <LibrarySearchField />
            <Tooltip
              content={
                <>
                  Or drop files anywhere <Kbd className="bg-bg/20 text-bg">U</Kbd>
                </>
              }
            >
              <Button onClick={pick}>
                <UploadIcon />
                Upload
              </Button>
            </Tooltip>
          </>
        }
      />

      {query.isPending ? (
        <LibraryLoading view={prefs.view} />
      ) : query.isError ? (
        <p
          role="alert"
          className="rounded-lg border border-danger/30 bg-danger-soft px-4 py-3 text-sm text-danger"
        >
          Couldn't load your library.{" "}
          {isApiError(query.error) ? query.error.message : "Try again in a moment."}
        </p>
      ) : files.length === 0 && !filtered ? (
        <LibraryEmpty onUpload={pick} />
      ) : (
        <>
          <LibraryToolbar
            search={search}
            onSearchChange={(next) =>
              void navigate({ to: "/library", search: next, replace: true })
            }
            sort={prefs.sort}
            onSortChange={(sort) => setPrefs({ sort })}
            view={prefs.view}
            onViewChange={(view) => setPrefs({ view })}
          />
          {selected.size > 0 ? (
            <SelectionBar
              count={selected.size}
              total={files.length}
              onSelectAll={() => setSelected(new Set(files.map((f) => f.id)))}
              onClear={() => setSelected(new Set())}
              onDelete={() => setDeleting([...selected])}
              onAsk={() =>
                void navigate({
                  to: "/chat",
                  search: { files: [...selected].slice(0, 100).join(",") },
                })
              }
            />
          ) : null}
          {files.length === 0 ? (
            <NoMatches
              onClear={() => void navigate({ to: "/library", search: {}, replace: true })}
            />
          ) : (
            <FileCollection
              files={files}
              view={prefs.view}
              selected={selected}
              selecting={selected.size > 0}
              onSelect={select}
              onTogglePin={togglePin}
              onRetry={retry}
              onDeleteRequest={setDeleting}
              onClearSelection={() => setSelected(new Set())}
            />
          )}
          <LoadMore
            hasMore={query.hasNextPage}
            loading={query.isFetchingNextPage}
            onLoad={() => void query.fetchNextPage()}
          />
        </>
      )}

      <ConfirmDeleteDialog
        open={deleting !== null}
        onOpenChange={(open) => {
          if (!open) setDeleting(null);
        }}
        ids={deleting ?? []}
        name={deletingName}
        onDeleted={() => setSelected(new Set())}
      />
    </div>
  );
}

/** Loads the next page when it scrolls into view; the button is the fallback. */
function LoadMore({
  hasMore,
  loading,
  onLoad,
}: {
  hasMore: boolean;
  loading: boolean;
  onLoad: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const onLoadRef = useRef(onLoad);
  onLoadRef.current = onLoad;
  useEffect(() => {
    const node = ref.current;
    if (!node || !hasMore || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) onLoadRef.current();
      },
      { rootMargin: "600px 0px" },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [hasMore]);

  if (!hasMore) return null;
  return (
    <div ref={ref} className="flex justify-center py-4">
      <Button variant="secondary" onClick={onLoad} disabled={loading}>
        {loading ? "Loading…" : "Load more"}
      </Button>
    </div>
  );
}

function LibrarySearchField() {
  const navigate = useNavigate();
  const [q, setQ] = useState("");
  return (
    // biome-ignore lint/a11y/useSemanticElements: <search> is not known to jsdom (tests)
    <form
      role="search"
      className="relative hidden sm:block"
      onSubmit={(event) => {
        event.preventDefault();
        void navigate({ to: "/search", search: q.trim() ? { q: q.trim() } : {} });
      }}
    >
      <SearchIcon
        className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-fg-subtle"
        aria-hidden
      />
      <Input
        data-search-field
        type="search"
        value={q}
        onChange={(e) => setQ(e.currentTarget.value)}
        placeholder="Search your library"
        aria-label="Search your library"
        className="h-9 w-56 pr-8 pl-9"
      />
      <Kbd className="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2">/</Kbd>
    </form>
  );
}
