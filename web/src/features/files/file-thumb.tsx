import { useState } from "react";
import { thumbnailUrl } from "@/api/files";
import { cn } from "@/lib/utils";
import { hasThumbnail, kindOf } from "./kind";

interface FileThumbProps {
  id: string;
  mime: string;
  status: string;
  /** `tile`: grid card; `icon`: small square in rows and lists. */
  variant?: "tile" | "icon";
  className?: string;
}

/** The server thumbnail for images, else a quiet type placeholder. */
export function FileThumb({ id, mime, status, variant = "tile", className }: FileThumbProps) {
  const kind = kindOf(mime);
  // Keyed by status: a thumbnail that 404'd while processing is retried once ready.
  const [failedFor, setFailedFor] = useState<string | null>(null);
  const showImage = hasThumbnail(mime) && failedFor !== status;
  const Icon = kind.icon;

  return (
    <div
      className={cn(
        "relative grid place-items-center overflow-hidden bg-surface-2 text-fg-subtle",
        variant === "tile" ? "aspect-[4/3] w-full" : "size-10 shrink-0 rounded-md",
        className,
      )}
    >
      {showImage ? (
        <img
          key={status}
          src={thumbnailUrl(id)}
          alt=""
          loading="lazy"
          decoding="async"
          draggable={false}
          onError={() => setFailedFor(status)}
          className="absolute inset-0 size-full object-cover"
        />
      ) : variant === "tile" ? (
        <div className="flex flex-col items-center gap-2" aria-hidden>
          <span className="grid size-12 place-items-center rounded-lg border border-border bg-surface shadow-xs">
            <Icon className="size-5" strokeWidth={1.5} />
          </span>
          <span className="font-mono text-2xs tracking-wider text-fg-subtle uppercase">
            {kind.short}
          </span>
        </div>
      ) : (
        <Icon className="size-4" strokeWidth={1.6} aria-hidden />
      )}
    </div>
  );
}
