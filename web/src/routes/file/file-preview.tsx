import { useQuery } from "@tanstack/react-query";
import { ExternalLinkIcon } from "lucide-react";
import { unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { downloadUrl, type FileItem } from "@/api/files";
import { Skeleton } from "@/components/ui/skeleton";
import { FileThumb } from "@/features/files/file-thumb";
import { categoryOf } from "@/features/files/kind";
import { formatBytes } from "@/lib/format";
import { Markdown } from "@/lib/markdown";

/** Text files up to this size are previewed from their bytes; longer ones via "Text". */
const TEXT_PREVIEW_MAX = 1024 * 1024;

function prettyJson(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

function TextPreview({ file }: { file: FileItem }) {
  const api = useApi();
  const query = useQuery({
    queryKey: ["files", "raw", file.id, file.content_hash],
    queryFn: ({ signal }) =>
      unwrap(
        api.GET("/api/v1/files/{id}/download", {
          params: { path: { id: file.id } },
          parseAs: "text",
          signal,
        }),
      ) as Promise<string>,
    staleTime: Number.POSITIVE_INFINITY,
  });
  if (query.isPending) {
    return (
      <div className="grid gap-2 p-6" aria-busy="true">
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-5/6" />
      </div>
    );
  }
  if (query.isError) {
    return <p className="p-6 text-sm text-danger">Couldn't load the text.</p>;
  }
  const text = query.data;
  if (file.mime_type === "text/markdown") {
    return (
      <div className="mx-auto max-w-[var(--reading-max)] px-5 py-6 sm:px-8 sm:py-8">
        <Markdown source={text} />
      </div>
    );
  }
  return (
    <pre className="max-h-[70vh] overflow-auto p-5 font-mono text-xs leading-relaxed whitespace-pre-wrap text-fg [overflow-wrap:anywhere] sm:p-6">
      {file.mime_type === "application/json" ? prettyJson(text) : text}
    </pre>
  );
}

/** The file itself: image, PDF viewer, player, or rendered text. */
export function FilePreview({ file, page }: { file: FileItem; page?: number }) {
  const category = categoryOf(file.mime_type);
  const frame = "overflow-hidden rounded-lg border border-border bg-surface shadow-xs";

  switch (category) {
    case "image":
      return (
        <div className={`${frame} grid place-items-center bg-surface-2 p-3 sm:p-6`}>
          <img
            src={downloadUrl(file.id)}
            alt={file.summary ?? file.name}
            className="max-h-[70vh] w-auto max-w-full rounded-sm object-contain shadow-sm"
          />
        </div>
      );
    case "pdf":
      return (
        <div className={frame}>
          <iframe
            key={page ?? 0}
            src={`${downloadUrl(file.id, true)}${page ? `#page=${page}` : ""}`}
            title={`PDF preview of ${file.name}`}
            className="block h-[75vh] w-full bg-surface-2"
          />
          <p className="flex items-center justify-between gap-3 border-t border-border px-4 py-2 text-xs text-fg-subtle">
            <span>Shown by your browser's PDF viewer.</span>
            <a
              href={downloadUrl(file.id, true)}
              target="_blank"
              rel="noopener"
              className="inline-flex items-center gap-1 font-medium text-accent-text hover:underline"
            >
              Open in a new tab <ExternalLinkIcon className="size-3" aria-hidden />
            </a>
          </p>
        </div>
      );
    case "audio":
      return (
        <div className={`${frame} grid gap-4 p-6`}>
          <FileThumb
            id={file.id}
            mime={file.mime_type}
            status={file.status}
            className="aspect-[5/2] rounded-md"
          />
          {/* biome-ignore lint/a11y/useMediaCaption: personal recordings have no captions */}
          <audio controls preload="metadata" src={downloadUrl(file.id)} className="w-full">
            Your browser can't play this audio.
          </audio>
        </div>
      );
    case "video":
      return (
        <div className={`${frame} bg-black`}>
          {/* biome-ignore lint/a11y/useMediaCaption: personal recordings have no captions */}
          <video
            controls
            preload="metadata"
            src={downloadUrl(file.id)}
            className="max-h-[70vh] w-full"
          >
            Your browser can't play this video.
          </video>
        </div>
      );
    case "text":
      return (
        <div className={frame}>
          {file.size_bytes <= TEXT_PREVIEW_MAX ? (
            <TextPreview file={file} />
          ) : (
            <p className="p-6 text-sm text-fg-muted">
              This file is {formatBytes(file.size_bytes)}, too long to preview here. Read it under
              “Text”, or download it.
            </p>
          )}
        </div>
      );
    default:
      return (
        <div className={frame}>
          <FileThumb id={file.id} mime={file.mime_type} status={file.status} />
          <p className="border-t border-border px-4 py-3 text-sm text-fg-muted">
            No preview for this type. Download it to open it.
          </p>
        </div>
      );
  }
}
