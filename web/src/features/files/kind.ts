// What kind of file something is, from its detected MIME type.

import {
  FileAudioIcon,
  FileIcon,
  FileImageIcon,
  FileJsonIcon,
  FileSpreadsheetIcon,
  FileTextIcon,
  FileVideoIcon,
  type LucideIcon,
  NotebookTextIcon,
} from "lucide-react";
import type { FileCategory } from "@/api/files";

export function categoryOf(mime: string): FileCategory | null {
  if (mime === "application/pdf") return "pdf";
  if (mime.startsWith("image/")) return "image";
  if (mime.startsWith("audio/")) return "audio";
  if (mime.startsWith("video/")) return "video";
  if (mime.startsWith("text/") || mime === "application/json") return "text";
  return null;
}

export const CATEGORY_LABELS: Record<FileCategory, string> = {
  pdf: "PDFs",
  image: "Images",
  text: "Text",
  audio: "Audio",
  video: "Video",
};

interface Kind {
  /** Short label: "PDF", "Markdown", "PNG image". */
  label: string;
  /** Extension-like tag shown on placeholders: "PDF", "MD", "PNG". */
  short: string;
  icon: LucideIcon;
}

const KINDS: Record<string, Kind> = {
  "application/pdf": { label: "PDF", short: "PDF", icon: FileTextIcon },
  "text/plain": { label: "Text", short: "TXT", icon: FileTextIcon },
  "text/markdown": { label: "Markdown", short: "MD", icon: NotebookTextIcon },
  "text/csv": { label: "CSV", short: "CSV", icon: FileSpreadsheetIcon },
  "application/json": { label: "JSON", short: "JSON", icon: FileJsonIcon },
};

export function kindOf(mime: string): Kind {
  const known = KINDS[mime];
  if (known) return known;
  const sub = (mime.split("/")[1] ?? "").replace(/^x-/, "").toUpperCase();
  switch (categoryOf(mime)) {
    case "image":
      return { label: `${sub} image`, short: sub, icon: FileImageIcon };
    case "audio":
      return { label: `${sub} audio`, short: sub, icon: FileAudioIcon };
    case "video":
      return {
        label: `${sub} video`,
        short: sub === "QUICKTIME" ? "MOV" : sub,
        icon: FileVideoIcon,
      };
    case "text":
      return { label: "Text", short: sub || "TXT", icon: FileTextIcon };
    default:
      return { label: "File", short: sub || "FILE", icon: FileIcon };
  }
}

/** Images get a server-made thumbnail; everything else shows its type. */
export function hasThumbnail(mime: string): boolean {
  return categoryOf(mime) === "image";
}
