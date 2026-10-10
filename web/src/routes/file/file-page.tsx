import { useQuery, useQueryClient } from "@tanstack/react-query";
import { getRouteApi, Link, useNavigate } from "@tanstack/react-router";
import {
  ArrowLeftIcon,
  CircleAlertIcon,
  DownloadIcon,
  EllipsisIcon,
  ExternalLinkIcon,
  FileQuestionIcon,
  PencilIcon,
  PinIcon,
  RotateCwIcon,
  SparklesIcon,
  Trash2Icon,
} from "lucide-react";
import { type ReactNode, useEffect, useId, useState } from "react";
import { isApiError } from "@/api/client";
import { useApi } from "@/api/context";
import { downloadUrl, type FileDetail, fileKeys, fileQuery } from "@/api/files";
import { metaQuery } from "@/api/queries";
import { EmptyState } from "@/components/common/empty-state";
import { isTyping } from "@/components/shell/use-global-shortcuts";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { toast } from "@/components/ui/toast";
import { ConfirmDeleteDialog } from "@/features/files/confirm-delete-dialog";
import { categoryOf, kindOf } from "@/features/files/kind";
import {
  enrichErrorMessage,
  useEnrich,
  useReindex,
  useTogglePin,
} from "@/features/files/mutations";
import { parsePassage } from "@/features/files/passage";
import { StatusBadge } from "@/features/files/status-badge";
import { useMarkOpened } from "@/features/files/use-mark-opened";
import { formatBytes, formatDateTime, formatRelative } from "@/lib/format";
import { useDocumentTitle } from "@/lib/use-document-title";
import { cn } from "@/lib/utils";
import { ExtractionViewer } from "./extraction-viewer";
import { FileCollections } from "./file-collections";
import { FilePreview } from "./file-preview";
import { ProcessingTimeline } from "./processing-timeline";
import { RenameDialog } from "./rename-dialog";
import { SimilarFiles } from "./similar-files";
import { TagsEditor } from "./tags-editor";

const route = getRouteApi("/app/files/$fileId");
const INLINE_TYPES = new Set(["pdf", "image", "audio", "video"]);

export function FilePage() {
  const { fileId } = route.useParams();
  const api = useApi();
  const query = useQuery(fileQuery(api, fileId));
  useDocumentTitle(query.data?.name ?? (query.isError ? "File not found" : "File"));

  if (query.isPending) return <FileSkeleton />;
  if (query.isError) {
    const missing =
      isApiError(query.error) && (query.error.status === 404 || query.error.status === 400);
    return (
      <EmptyState
        icon={FileQuestionIcon}
        title={missing ? "This file isn't in your library" : "Couldn't open this file"}
        actions={
          <Button variant="secondary" asChild>
            <Link to="/library">Back to the library</Link>
          </Button>
        }
      >
        <p>
          {missing ? "It may have been deleted, or the link is wrong." : "Try again in a moment."}
        </p>
      </EmptyState>
    );
  }
  return <FileView file={query.data} />;
}

function FileView({ file }: { file: FileDetail }) {
  const api = useApi();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const chatModel = useQuery(metaQuery(api)).data?.chat_model ?? false;
  const [renaming, setRenaming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const togglePin = useTogglePin();
  const reindex = useReindex();
  const enrich = useEnrich();
  const [enrichedAt, setEnrichedAt] = useState(0);
  const { at, page } = route.useSearch();
  const passage = parsePassage(at);
  // A link to a passage (search result, citation) opens the text at that passage.
  const [tab, setTab] = useState(passage ? "text" : "preview");
  useEffect(() => {
    if (at) setTab("text");
  }, [at]);
  const kind = kindOf(file.mime_type);
  const category = categoryOf(file.mime_type);
  useMarkOpened(file.id);

  // After asking for a new summary, look again a few times while the job runs.
  useEffect(() => {
    if (!enrichedAt) return;
    const timers = [3000, 8000, 15000, 30000].map((ms) =>
      window.setTimeout(
        () => void queryClient.invalidateQueries({ queryKey: fileKeys.detail(file.id) }),
        ms,
      ),
    );
    return () => timers.forEach(window.clearTimeout);
  }, [enrichedAt, file.id, queryClient]);

  // Delete key asks to delete this file.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Delete" || isTyping(event.target)) return;
      if (document.querySelector("[role=dialog]")) return;
      event.preventDefault();
      setDeleting(true);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const retry = () =>
    reindex.mutate(file.id, {
      onSuccess: () => toast({ title: "Reading it again", tone: "success" }),
      onError: (e) =>
        toast({
          title: "Couldn't start",
          description: isApiError(e) ? e.message : undefined,
          tone: "danger",
        }),
    });
  const regenerate = () =>
    enrich.mutate(file.id, {
      onSuccess: () => {
        setEnrichedAt(Date.now());
        toast({
          title: "Writing a new summary",
          description: "It appears here in a moment.",
          tone: "success",
        });
      },
      onError: (e) =>
        toast({ title: "Couldn't regenerate", description: enrichErrorMessage(e), tone: "danger" }),
    });

  return (
    <div className="grid gap-6">
      <Link
        to="/library"
        className="inline-flex w-fit items-center gap-1.5 text-xs font-medium text-fg-muted hover:text-fg"
      >
        <ArrowLeftIcon className="size-3.5" aria-hidden /> Library
      </Link>

      <header className="flex flex-col gap-4 border-b border-border pb-6 lg:flex-row lg:items-end lg:justify-between">
        <div className="grid min-w-0 gap-1.5">
          <p className="eyebrow">
            {kind.label} · {formatBytes(file.size_bytes)}
          </p>
          <h1 className="display text-3xl break-words text-fg sm:text-4xl">{file.name}</h1>
          <p className="flex flex-wrap items-center gap-2 text-sm text-fg-muted">
            <span>
              Added{" "}
              <time dateTime={file.created_at} title={formatDateTime(file.created_at)}>
                {formatRelative(file.created_at)}
              </time>
            </span>
            <StatusBadge status={file.status} showReady />
          </p>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-2">
          <Button
            variant="secondary"
            aria-pressed={file.is_pinned}
            onClick={() => togglePin(file)}
            className={cn(
              file.is_pinned &&
                "border-accent/40 bg-accent-soft text-accent-text hover:bg-accent-soft",
            )}
          >
            <PinIcon className={cn(file.is_pinned && "fill-current")} />
            {file.is_pinned ? "Pinned" : "Pin"}
          </Button>
          <Button variant="secondary" asChild>
            <a href={downloadUrl(file.id)} download>
              <DownloadIcon /> Download
            </a>
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="secondary" size="icon" aria-label="More actions">
                <EllipsisIcon />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={() => setRenaming(true)}>
                <PencilIcon /> Rename
              </DropdownMenuItem>
              {category && INLINE_TYPES.has(category) ? (
                <DropdownMenuItem asChild>
                  <a href={downloadUrl(file.id, true)} target="_blank" rel="noopener">
                    <ExternalLinkIcon /> Open in a new tab
                  </a>
                </DropdownMenuItem>
              ) : null}
              <DropdownMenuItem onSelect={retry}>
                <RotateCwIcon /> Read it again
              </DropdownMenuItem>
              {chatModel ? (
                <DropdownMenuItem onSelect={regenerate} disabled={file.status !== "ready"}>
                  <SparklesIcon /> New summary and tags
                </DropdownMenuItem>
              ) : null}
              <DropdownMenuSeparator />
              <DropdownMenuItem tone="danger" onSelect={() => setDeleting(true)}>
                <Trash2Icon /> Delete…
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </header>

      {file.status === "failed" ? (
        <div
          role="alert"
          className="flex flex-col gap-3 rounded-lg border border-danger/30 bg-danger-soft px-4 py-3 sm:flex-row sm:items-center"
        >
          <CircleAlertIcon className="size-4 shrink-0 text-danger" aria-hidden />
          <p className="flex-1 text-sm text-fg">
            <span className="font-medium text-danger">Akasha couldn't read this file.</span>{" "}
            {file.error ?? "Something went wrong while processing it."}
          </p>
          <Button variant="secondary" size="sm" onClick={retry} disabled={reindex.isPending}>
            <RotateCwIcon /> Retry
          </Button>
        </div>
      ) : null}

      <div className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_19rem]">
        <Tabs value={tab} onValueChange={setTab} className="min-w-0">
          <TabsList className="w-full">
            <TabsTrigger value="preview">Preview</TabsTrigger>
            <TabsTrigger value="text">Text</TabsTrigger>
          </TabsList>
          <TabsContent value="preview">
            <FilePreview file={file} page={page} />
          </TabsContent>
          <TabsContent value="text">
            <ExtractionViewer file={file} passage={passage} />
          </TabsContent>
        </Tabs>

        <aside className="grid content-start gap-7" aria-label="About this file">
          <Section title="Summary">
            {file.summary ? (
              <p className="font-display text-base leading-relaxed text-fg">{file.summary}</p>
            ) : (
              <p className="text-xs text-fg-subtle">
                {chatModel
                  ? file.status === "ready"
                    ? "No summary yet."
                    : "A short description appears once the file has been read."
                  : "Summaries need a language model; none is configured on this server."}
              </p>
            )}
          </Section>
          <Section title="Tags">
            <TagsEditor file={file} />
          </Section>
          <Section title="Collections">
            <FileCollections file={file} />
          </Section>
          <Section title="Processing">
            <ProcessingTimeline file={file} chatModel={chatModel} />
          </Section>
          <Section title="Details">
            <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5 text-xs">
              <dt className="text-fg-subtle">Type</dt>
              <dd className="text-fg">{file.mime_type}</dd>
              <dt className="text-fg-subtle">Size</dt>
              <dd className="text-fg">{formatBytes(file.size_bytes)}</dd>
              <dt className="text-fg-subtle">Added</dt>
              <dd className="text-fg">{formatDateTime(file.created_at)}</dd>
              <dt className="text-fg-subtle">Changed</dt>
              <dd className="text-fg">{formatDateTime(file.updated_at)}</dd>
              <dt className="text-fg-subtle">SHA-256</dt>
              <dd className="truncate font-mono text-fg-muted" title={file.content_hash}>
                {file.content_hash.slice(0, 16)}…
              </dd>
            </dl>
          </Section>
          <Section title="Similar files">
            <SimilarFiles file={file} />
          </Section>
        </aside>
      </div>

      <RenameDialog file={file} open={renaming} onOpenChange={setRenaming} />
      <ConfirmDeleteDialog
        open={deleting}
        onOpenChange={setDeleting}
        ids={[file.id]}
        name={file.name}
        onDeleted={() => void navigate({ to: "/library", replace: true })}
      />
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  const id = useId();
  return (
    <section aria-labelledby={id} className="grid gap-3">
      <h2 id={id} className="eyebrow flex items-center gap-3">
        {title}
        <span className="h-px flex-1 bg-border" aria-hidden />
      </h2>
      {children}
    </section>
  );
}

function FileSkeleton() {
  return (
    <div className="grid gap-6" role="status" aria-busy="true" aria-label="Loading file">
      <Skeleton className="h-4 w-20" />
      <div className="grid gap-2 border-b border-border pb-6">
        <Skeleton className="h-3 w-32" />
        <Skeleton className="h-10 w-2/3" />
      </div>
      <div className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_19rem]">
        <Skeleton className="h-[50vh]" />
        <div className="grid content-start gap-4">
          <Skeleton className="h-20" />
          <Skeleton className="h-16" />
          <Skeleton className="h-32" />
        </div>
      </div>
    </div>
  );
}
