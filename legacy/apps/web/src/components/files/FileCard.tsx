import { useState, useEffect } from "react";
import {
  FileText,
  Image as ImageIcon,
  FileCode,
  File,
  Download,
  Trash2,
  Loader2,
  Check,
  AlertTriangle,
  Play,
  Star,
} from "lucide-react";
import { api } from "../../lib/api";
import { useAuthStore } from "../../store/auth.store";
import { useUIStore } from "../../store/ui.store";
import { toast } from "sonner";

export interface FileItem {
  id: string;
  originalName: string;
  mimeType: string;
  sizeBytes: number;
  status: "pending" | "processing" | "completed" | "failed";
  createdAt: string;
  tags?: string[] | null;
  summary?: string | null;
  isStarred?: boolean;
  isPinned?: boolean;
  lastOpenedAt?: string | null;
  openCount?: number;
  chunkCount?: number;
  collectionId?: string | null;
}

interface FileCardProps {
  file: FileItem;
  onOpen: () => void;
  onDownload: () => void;
  onDelete: () => void;
  onToggleStar?: () => void;
  onTogglePin?: () => void;
  onRename?: (newName: string) => Promise<void>;
  isDownloading?: boolean;
  isDeleting?: boolean;
  isStarring?: boolean;
  isFocused?: boolean;
  isSelected?: boolean;
  onSelectToggle?: (e: React.MouseEvent) => void;
  onTagClick?: (tag: string) => void;
  collections?: Array<{ id: string; name: string; color: string }>;
  onMoveToCollection?: (colId: string) => void;
}

// Format file size helper
export function formatBytes(bytes: number, decimals = 2) {
  if (bytes === 0) return "0 Bytes";
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ["Bytes", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + " " + sizes[i];
}

// Zero-dependency relative time formatter
export function getRelativeTimeString(dateString: string): string {
  const date = new Date(dateString);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffSec = Math.floor(diffMs / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHr = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHr / 24);

  if (diffSec < 60) return "just now";
  if (diffMin < 60) return `${diffMin}m ago`;
  if (diffHr < 24) return `${diffHr}h ago`;
  if (diffDay === 1) return "yesterday";
  if (diffDay < 30) return `${diffDay}d ago`;
  
  const diffMonth = Math.floor(diffDay / 30);
  if (diffMonth < 12) return `${diffMonth}mo ago`;
  
  return date.toLocaleDateString(undefined, { dateStyle: "short" });
}

// Color-coded file icon mapping
export function getFileTypeIcon(mimeType: string) {
  const c = "h-5 w-5 shrink-0";
  if (mimeType.includes("pdf")) return <FileText className={`${c} text-red-400`} />;
  if (mimeType.includes("image")) return <ImageIcon className={`${c} text-purple-400`} />;
  if (mimeType.includes("text/plain") || mimeType.includes("markdown") || mimeType.includes("csv")) return <FileCode className={`${c} text-blue-400`} />;
  if (mimeType.includes("video") || mimeType.includes("audio")) return <Play className={`${c} text-amber-400`} />;
  return <File className={`${c} text-text-muted`} />;
}

// Color-coded badge helper
export function getFileTypeBadge(mimeType: string) {
  let label = "File";
  let classes = "bg-text-dim/10 border-text-dim/20 text-text-muted";
  
  if (mimeType.includes("pdf")) {
    label = "PDF";
    classes = "bg-red-400/10 border-red-400/20 text-red-400";
  } else if (mimeType.includes("image")) {
    label = "Image";
    classes = "bg-purple-400/10 border-purple-400/20 text-purple-400";
  } else if (mimeType.includes("text/plain") || mimeType.includes("markdown") || mimeType.includes("csv")) {
    label = mimeType.includes("csv") ? "CSV" : (mimeType.includes("markdown") ? "Markdown" : "Text");
    classes = "bg-blue-400/10 border-blue-400/20 text-blue-400";
  } else if (mimeType.includes("video") || mimeType.includes("audio")) {
    label = mimeType.includes("video") ? "Video" : "Audio";
    classes = "bg-amber-400/10 border-amber-400/20 text-amber-400";
  }
  
  return (
    <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium tracking-wider border select-none ${classes}`}>
      {label}
    </span>
  );
}

// Calm status badge helper
export function getStatusBadge(status: string) {
  const base = "inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs font-semibold uppercase tracking-wider border select-none";
  switch (status) {
    case "pending":
      return (
        <span className={`${base} bg-bg-base border-border-default text-text-dim`}>
          <span className="h-1.5 w-1.5 rounded-full bg-text-dim animate-pulse"></span>
          Pending
        </span>
      );
    case "processing":
      return (
        <span className={`${base} bg-state-processing/10 border-state-processing/20 text-state-processing`}>
          <span className="h-1.5 w-1.5 rounded-full bg-state-processing animate-pulse"></span>
          Processing
        </span>
      );
    case "completed":
      return (
        <span className={`${base} bg-state-success/10 border-state-success/20 text-state-success`}>
          <Check className="h-3 w-3 shrink-0" />
          Indexed
        </span>
      );
    case "failed":
      return (
        <span className={`${base} bg-state-error/10 border-state-error/20 text-state-error`}>
          <AlertTriangle className="h-3 w-3 shrink-0" />
          Failed
        </span>
      );
    default:
      return null;
  }
}

export function FileCard({
  file,
  onOpen,
  onDownload,
  onDelete,
  onToggleStar,
  onTogglePin,
  onRename,
  isDownloading = false,
  isDeleting = false,
  isStarring = false,
  isFocused = false,
  isSelected = false,
  onSelectToggle,
  onTagClick,
  collections = [],
  onMoveToCollection,
}: FileCardProps) {
  const visibleTags = file.tags?.slice(0, 3) || [];
  const hiddenTagsCount = (file.tags?.length || 0) - visibleTags.length;

  const [isEditing, setIsEditing] = useState(false);
  const [editName, setEditName] = useState(file.originalName);
  const [thumbnailSrc, setThumbnailSrc] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);

  useEffect(() => {
    if (!contextMenu) return;
    const handleClose = () => setContextMenu(null);
    window.addEventListener("click", handleClose);
    return () => window.removeEventListener("click", handleClose);
  }, [contextMenu]);

  const { accessToken } = useAuthStore();
  const { setDraggedFileId } = useUIStore();

  // Load dynamically generated authenticated image thumbnails
  useEffect(() => {
    let active = true;
    if (file.mimeType.startsWith("image/") && file.status === "completed") {
      const loadThumbnail = async () => {
        try {
          const response = await fetch(`/api/v1/files/${file.id}/thumbnail`, {
            headers: {
              Authorization: `Bearer ${accessToken}`,
            },
          });
          if (!response.ok) throw new Error();
          const blob = await response.blob();
          if (active) {
            setThumbnailSrc(URL.createObjectURL(blob));
          }
        } catch (e) {
          // Fallback to default icon on error
        }
      };
      loadThumbnail();
    }
    return () => {
      active = false;
      if (thumbnailSrc) {
        URL.revokeObjectURL(thumbnailSrc);
      }
    };
  }, [file.id, file.status, file.mimeType, accessToken]);

  // Listen to F2 (rename) and S (star) keyboard triggers when card is focused
  useEffect(() => {
    if (!isFocused) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (
        document.activeElement?.tagName === "INPUT" ||
        document.activeElement?.tagName === "TEXTAREA"
      ) {
        return;
      }
      if (e.key === "F2") {
        e.preventDefault();
        setIsEditing(true);
        setEditName(file.originalName);
      } else if (e.key.toLowerCase() === "s") {
        e.preventDefault();
        if (onToggleStar) onToggleStar();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isFocused, file.originalName, onToggleStar]);

  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsEditing(true);
    setEditName(file.originalName);
  };

  const handleKeyDown = async (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.stopPropagation();
      const trimmed = editName.trim();
      if (trimmed && trimmed !== file.originalName) {
        try {
          await api.patch(`/api/v1/files/${file.id}`, { originalName: trimmed });
          if (onRename) {
            await onRename(trimmed);
          }
          toast.success("File renamed.");
        } catch (err: any) {
          toast.error(err.message || "Failed to rename file.");
          console.error("Rename failed", err);
        }
      }
      setIsEditing(false);
    } else if (e.key === "Escape") {
      e.stopPropagation();
      setIsEditing(false);
      setEditName(file.originalName);
    }
  };

  return (
    <div
      onClick={onOpen}
      draggable={true}
      onDragStart={(e) => {
        setDraggedFileId(file.id);
        e.dataTransfer.effectAllowed = "move";
        e.dataTransfer.setData("text/plain", file.id);
      }}
      onDragEnd={() => {
        setDraggedFileId(null);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        setContextMenu({ x: e.clientX, y: e.clientY });
      }}
      className={`bg-bg-surface border rounded-xl p-6 relative group select-none cursor-pointer flex flex-col justify-start hover:bg-bg-surface-raised transition-all duration-[120ms] ease-motion ${
        isFocused 
          ? "border-accent-primary ring-2 ring-accent-primary/20 scale-[1.01] shadow-lg" 
          : "border-border-default hover:border-border-strong"
      }`}
    >
      {/* Checkbox Selector overlay for multi-select actions */}
      {onSelectToggle && (
        <div 
          onClick={(e) => {
            e.stopPropagation();
            onSelectToggle(e);
          }}
          className={`absolute top-4 left-4 h-4 w-4 border rounded flex items-center justify-center transition-all duration-[120ms] ease-motion cursor-pointer z-20 ${
            isSelected 
              ? "bg-accent-primary border-accent-primary text-bg-base" 
              : "border-border-default hover:border-border-strong opacity-0 group-hover:opacity-100 bg-bg-surface"
          }`}
        >
          {isSelected && <Check className="h-3 w-3 stroke-[3] text-bg-base" />}
        </div>
      )}

      <div className="flex items-start gap-4">
        {/* Color-coded Icon Box / Authenticated Thumbnail preview */}
        <div className={`h-12 w-12 rounded bg-bg-base border border-border-default overflow-hidden flex items-center justify-center shrink-0 transition-all duration-[120ms] ease-motion group-hover:border-border-strong group-hover:bg-bg-surface-raised relative ${onSelectToggle ? "ml-6" : ""}`}>
          {thumbnailSrc ? (
            <img 
              src={thumbnailSrc} 
              alt={file.originalName} 
              className="h-full w-full object-cover transition-transform duration-[120ms] group-hover:scale-105"
            />
          ) : (
            getFileTypeIcon(file.mimeType)
          )}
        </div>

        {/* Text Details Area */}
        <div className="min-w-0 flex-1 space-y-2 pr-16">
          {isEditing ? (
            <input
              type="text"
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              onKeyDown={handleKeyDown}
              onBlur={() => setIsEditing(false)}
              onClick={(e) => e.stopPropagation()}
              autoFocus
              className="w-full bg-bg-base border border-accent-primary focus:outline-none rounded px-2 py-0.5 text-sm text-text-primary font-sans font-medium"
            />
          ) : (
            <div 
              className="flex items-center gap-2 min-w-0" 
              onDoubleClick={handleDoubleClick}
              title="Double-click to rename"
            >
              <h4
                className="font-sans text-base font-medium tracking-tight text-text-primary group-hover:text-accent-hover transition-colors truncate selection:bg-accent-primary/20 select-text"
                title={file.originalName}
              >
                {file.originalName}
              </h4>
              {file.isStarred && (
                <Star className="h-3.5 w-3.5 text-amber-400 fill-amber-400 shrink-0 select-none animate-fade-in" />
              )}
            </div>
          )}
          <p className="text-xs text-text-muted">
            {formatBytes(file.sizeBytes)} · {getRelativeTimeString(file.createdAt)}
            {file.chunkCount !== undefined && file.chunkCount > 0 && ` · ${file.chunkCount} chunk${file.chunkCount === 1 ? "" : "s"}`}
            {file.openCount !== undefined && file.openCount > 0 && ` · ${file.openCount} open${file.openCount === 1 ? "" : "s"}`}
          </p>

          {file.summary && (
            <p className="text-xs text-text-muted leading-relaxed line-clamp-2 mt-1 select-text" title={file.summary}>
              {file.summary}
            </p>
          )}

          <div className="flex flex-wrap items-center gap-1.5">
            {getFileTypeBadge(file.mimeType)}
            {getStatusBadge(file.status)}
          </div>

          {/* Auto-tags inline block */}
          {file.tags && file.tags.length > 0 && (
            <div className="flex flex-wrap items-center gap-1.5 pt-2 border-t border-border-default/50 mt-1">
              {visibleTags.map((tag) => (
                <button
                  key={tag}
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    onTagClick?.(tag);
                  }}
                  className="px-2 py-0.5 rounded bg-bg-base border border-border-default hover:border-border-strong text-text-muted hover:text-text-primary text-xs font-medium truncate max-w-[110px] cursor-pointer transition-all hover:bg-bg-surface-raised"
                >
                  {tag}
                </button>
              ))}
              {hiddenTagsCount > 0 && (
                <span className="px-2 py-0.5 rounded bg-accent-subtle border border-accent-subtle-border text-accent-primary text-xs font-bold">
                  +{hiddenTagsCount}
                </span>
              )}
            </div>
          )}

          {isFocused && (
            <div className="flex items-center gap-1.5 pt-2 border-t border-border-default/30 mt-2 text-[10px] font-mono text-text-dim select-none animate-fade-in">
              <span>Enter open · D download · S star · Del delete</span>
            </div>
          )}
        </div>
      </div>

      {/* Floating Card Action Controls */}
      <div className="absolute top-4 right-4 flex items-center gap-1.5 opacity-0 group-hover:opacity-100 transition-opacity duration-200 pointer-events-none group-hover:pointer-events-auto">
        <button
          onClick={(e) => {
            e.stopPropagation();
            if (onToggleStar) onToggleStar();
          }}
          disabled={isStarring}
          className="p-1.5 rounded-lg bg-bg-surface border border-border-default text-text-muted hover:text-amber-400 hover:border-border-strong transition-all duration-[120ms] ease-motion cursor-pointer shadow-sm flex items-center justify-center"
          title={file.isStarred ? "Remove Star" : "Star File"}
        >
          <Star className={`h-3.5 w-3.5 ${file.isStarred ? "text-amber-400 fill-amber-400" : "text-text-muted"}`} />
        </button>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onDownload();
          }}
          disabled={isDownloading || isDeleting}
          className="p-1.5 rounded-lg bg-bg-surface border border-border-default text-text-primary hover:text-accent-primary hover:border-border-strong transition-all duration-[120ms] ease-motion disabled:opacity-50 cursor-pointer shadow-sm flex items-center justify-center"
          title="Download File"
        >
          {isDownloading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Download className="h-3.5 w-3.5" />
          )}
        </button>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
          disabled={isDownloading || isDeleting}
          className="p-1.5 rounded-lg bg-bg-surface border border-border-default text-state-error hover:bg-state-error/10 hover:border-state-error/25 transition-all duration-[120ms] ease-motion disabled:opacity-50 cursor-pointer shadow-sm flex items-center justify-center"
          title="Delete File"
        >
          {isDeleting ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Trash2 className="h-3.5 w-3.5" />
          )}
        </button>
      </div>

      {/* Indeterminate processing progress shimmer overlay */}
      {file.status === "processing" && (
        <div className="absolute bottom-0 left-0 right-0 h-[3px] bg-accent-primary/10 overflow-hidden rounded-b-xl">
          <div className="h-full bg-accent-primary w-1/3 rounded animate-shimmer-progress"></div>
        </div>
      )}

      {/* Floating Right-Click Desktop-grade Context Menu */}
      {contextMenu && (
        <div
          className="fixed z-50 w-52 bg-bg-surface/95 backdrop-blur-md border border-border-strong rounded-xl shadow-2xl p-1 animate-scale-in text-xs font-semibold text-text-primary select-none font-sans"
          style={{ top: contextMenu.y, left: contextMenu.x }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            onClick={() => {
              onOpen();
              setContextMenu(null);
            }}
            className="w-full text-left px-3 py-2 hover:bg-accent-subtle hover:text-accent-primary rounded-lg transition-colors flex items-center gap-2 cursor-pointer"
          >
            Open File
          </button>
          <button
            onClick={() => {
              onDownload();
              setContextMenu(null);
            }}
            className="w-full text-left px-3 py-2 hover:bg-accent-subtle hover:text-accent-primary rounded-lg transition-colors flex items-center gap-2 cursor-pointer"
          >
            Download File
          </button>
          <button
            onClick={() => {
              if (onToggleStar) onToggleStar();
              setContextMenu(null);
            }}
            className="w-full text-left px-3 py-2 hover:bg-accent-subtle hover:text-accent-primary rounded-lg transition-colors flex items-center gap-2 cursor-pointer"
          >
            {file.isStarred ? "Unstar File" : "Star File"}
          </button>
          <button
            onClick={() => {
              if (onTogglePin) onTogglePin();
              setContextMenu(null);
            }}
            className="w-full text-left px-3 py-2 hover:bg-accent-subtle hover:text-accent-primary rounded-lg transition-colors flex items-center gap-2 cursor-pointer"
          >
            {file.isPinned ? "Unpin from top" : "Pin to top"}
          </button>
          <button
            onClick={() => {
              setIsEditing(true);
              setEditName(file.originalName);
              setContextMenu(null);
            }}
            className="w-full text-left px-3 py-2 hover:bg-accent-subtle hover:text-accent-primary rounded-lg transition-colors flex items-center gap-2 cursor-pointer"
          >
            Rename File
          </button>

          {/* Move to Collection submenu */}
          {collections && collections.length > 0 && (
            <div className="relative group/sub">
              <button
                className="w-full text-left px-3 py-2 hover:bg-accent-subtle hover:text-accent-primary rounded-lg transition-colors flex items-center justify-between gap-2 cursor-pointer"
              >
                <span>Move to collection</span>
                <span className="text-[10px] text-text-dim">▶</span>
              </button>
              <div className="absolute left-full top-0 ml-1 hidden group-hover/sub:block w-48 bg-bg-surface/95 backdrop-blur-md border border-border-strong rounded-xl shadow-2xl p-1 animate-scale-in">
                {collections.map((col) => (
                  <button
                    key={col.id}
                    onClick={() => {
                      if (onMoveToCollection) onMoveToCollection(col.id);
                      setContextMenu(null);
                    }}
                    className="w-full text-left px-3 py-1.5 hover:bg-accent-subtle hover:text-accent-primary rounded-lg transition-colors flex items-center gap-2 truncate cursor-pointer text-[11px]"
                  >
                    <span className="h-2 w-2 rounded-full shrink-0" style={{ backgroundColor: col.color || "#3b82f6" }} />
                    <span className="truncate">{col.name}</span>
                  </button>
                ))}
              </div>
            </div>
          )}

          <button
            onClick={() => {
              navigator.clipboard.writeText(file.originalName);
              toast.success("Filename copied.");
              setContextMenu(null);
            }}
            className="w-full text-left px-3 py-2 hover:bg-accent-subtle hover:text-accent-primary rounded-lg transition-colors flex items-center gap-2 cursor-pointer border-t border-border-default/30 mt-1 pt-2"
          >
            Copy filename
          </button>
          
          {file.status === "completed" && (
            <button
              onClick={async () => {
                setContextMenu(null);
                try {
                  const res = await api.get(`/api/v1/files/${file.id}`);
                  const text = res?.data?.extractedText || "";
                  if (!text) {
                    toast.error("No text content available to export.");
                    return;
                  }
                  const content = `# ${file.originalName}\n\n${text}`;
                  const blob = new Blob([content], { type: "text/markdown" });
                  const url = URL.createObjectURL(blob);
                  const a = document.createElement("a");
                  a.href = url;
                  a.download = `${file.originalName.replace(/\.[^/.]+$/, "")}.md`;
                  a.click();
                  URL.revokeObjectURL(url);
                  toast.success("Markdown exported successfully.");
                } catch {
                  toast.error("Failed to export Markdown.");
                }
              }}
              className="w-full text-left px-3 py-2 hover:bg-accent-subtle hover:text-accent-primary rounded-lg transition-colors flex items-center gap-2 cursor-pointer"
            >
              Export as Markdown
            </button>
          )}

          <button
            onClick={() => {
              onDelete();
              setContextMenu(null);
            }}
            className="w-full text-left px-3 py-2 hover:bg-state-error/10 hover:text-state-error rounded-lg transition-colors flex items-center gap-2 cursor-pointer border-t border-border-default/30 mt-1 pt-2"
          >
            Delete File
          </button>
        </div>
      )}
    </div>
  );
}
