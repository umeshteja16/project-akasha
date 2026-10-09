import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api, ApiError } from "../../lib/api";
import { useAuthStore } from "../../store/auth.store";
import { useUIStore } from "../../store/ui.store";
import {
  X,
  Copy,
  Check,
  Download,
  FileDown,
  Loader2,
  AlertCircle,
  Tag,
  Plus,
  Trash2,
  ChevronRight,
  Brain,
  Play,
  Folder,
} from "lucide-react";
import React, { useState } from "react";
import { toast } from "sonner";
import { getFileTypeBadge, formatBytes, getFileTypeIcon } from "./FileCard";

interface ExtractionData {
  fileId: string;
  extractedText: string;
  meta?: {
    pages?: number;
    wordCount?: number;
    durationMs?: number;
    engine?: string;
  };
  createdAt: string;
  mimeType?: string;
  originalName?: string;
  fileCreatedAt?: string;
  tags?: string[];
  summary?: string;
  sizeBytes?: number;
  status?: string;
  collectionId?: string | null;
}

interface FileDetailSheetProps {
  fileId: string;
  fileName: string;
  isOpen: boolean;
  onClose: () => void;
  onDelete: () => void;
  onDownload: () => void;
  isDownloading?: boolean;
  isDeleting?: boolean;
}

export function FileDetailSheet({
  fileId,
  fileName,
  isOpen,
  onClose,
  onDelete,
  onDownload,
  isDownloading = false,
  isDeleting = false,
}: FileDetailSheetProps) {
  const [copied, setCopied] = useState(false);
  const [summaryExpanded, setSummaryExpanded] = useState(true);
  const [newTagInputVisible, setNewTagInputVisible] = useState(false);
  const [newTagValue, setNewTagValue] = useState("");
  const queryClient = useQueryClient();

  const { accessToken } = useAuthStore();
  const [blobUrl, setBlobUrl] = useState<string | null>(null);
  const [mediaLoading, setMediaLoading] = useState(false);
  const [mediaError, setMediaError] = useState<string | null>(null);
  const [fileContentText, setFileContentText] = useState<string | null>(null);
  const [forceTextView, setForceTextView] = useState(false);

  // Fetch the text extraction details (plus tags and summaries)
  const { data, isLoading, error } = useQuery<{ data: ExtractionData }>({
    queryKey: ["extraction", fileId],
    queryFn: () => api.get(`/api/v1/files/${fileId}/extraction`),
    enabled: isOpen && !!fileId,
  });

  const { setSelectedFile } = useUIStore();

  // Fetch semantically similar files using pgvector similarity
  const { data: similarData } = useQuery<{ data: Array<{ id: string; originalName: string; mimeType: string; sizeBytes: number; status: string; isStarred: boolean; similarity: number }> }>({
    queryKey: ["similar-files", fileId],
    queryFn: () => api.get(`/api/v1/files/${fileId}/similar`),
    enabled: isOpen && !!fileId,
  });
  const similarList = similarData?.data || [];

  // Fetch collections for folder picker dropdown
  const { data: collectionsData } = useQuery<{ data: Array<{ id: string; name: string; color: string }> }>({
    queryKey: ["collections"],
    queryFn: () => api.get("/api/v1/collections"),
    enabled: isOpen && !!fileId,
  });
  const collectionsList = collectionsData?.data || [];

  const { mutate: updateCollection } = useMutation({
    mutationFn: async (colId: string | null) => {
      await api.patch(`/api/v1/files/${fileId}`, { collectionId: colId });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["extraction", fileId] });
      queryClient.invalidateQueries({ queryKey: ["files"] });
      toast.success("Collection updated.");
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to update collection.");
    },
  });

  const mimeType = data?.data.mimeType || "";
  const isSupportedFormat = 
    mimeType.includes("pdf") ||
    mimeType.includes("image") ||
    mimeType.includes("markdown") ||
    mimeType.includes("x-markdown") ||
    mimeType.includes("csv") ||
    mimeType.includes("text/plain") ||
    mimeType.includes("video") ||
    mimeType.includes("audio");

  React.useEffect(() => {
    if (!isOpen || !fileId) {
      if (blobUrl) {
        URL.revokeObjectURL(blobUrl);
        setBlobUrl(null);
      }
      setFileContentText(null);
      setMediaError(null);
      setForceTextView(false);
      return;
    }

    const mime = data?.data.mimeType;
    if (!mime) return;

    // If format is unsupported and we are not forcing text view, do not fetch the large blob
    if (!isSupportedFormat && !forceTextView) {
      return;
    }

    const loadFileBlob = async () => {
      setMediaLoading(true);
      setMediaError(null);
      try {
        const response = await fetch(`/api/v1/files/${fileId}/download`, {
          headers: {
            Authorization: `Bearer ${accessToken}`,
          },
        });

        if (!response.ok) {
          throw new Error("Failed to load content stream.");
        }

        const blob = await response.blob();
        
        if (forceTextView || mime.includes("text/") || mime.includes("csv") || mime.includes("markdown")) {
          const text = await blob.text();
          setFileContentText(text);
        } else {
          const url = URL.createObjectURL(blob);
          setBlobUrl(url);
        }
      } catch (err: any) {
        setMediaError(err.message || "Failed to load file preview.");
      } finally {
        setMediaLoading(false);
      }
    };

    loadFileBlob();

    return () => {
      if (blobUrl) {
        URL.revokeObjectURL(blobUrl);
      }
    };
  }, [isOpen, fileId, data?.data.mimeType, accessToken, isSupportedFormat, forceTextView]);

  const renderMarkdown = (text: string) => {
    const lines = text.split("\n");
    return lines.map((line, idx) => {
      const trimmed = line.trim();
      if (trimmed.startsWith("### ")) {
        return <h5 key={idx} className="font-serif text-sm font-semibold text-text-primary mt-3 mb-1.5">{trimmed.slice(4)}</h5>;
      }
      if (trimmed.startsWith("## ")) {
        return <h4 key={idx} className="font-serif text-base font-semibold text-text-primary mt-4 mb-2">{trimmed.slice(3)}</h4>;
      }
      if (trimmed.startsWith("# ")) {
        return <h3 key={idx} className="font-serif text-lg font-semibold text-text-primary mt-5 mb-3 border-b border-border-default pb-1">{trimmed.slice(2)}</h3>;
      }
      if (trimmed.startsWith("- ") || trimmed.startsWith("* ")) {
        return <li key={idx} className="text-xs text-text-muted list-disc ml-5 mb-1 leading-relaxed font-sans font-medium">{trimmed.slice(2)}</li>;
      }
      if (trimmed.startsWith("> ")) {
        return <blockquote key={idx} className="border-l-2 border-accent-primary pl-3 py-1 italic text-xs text-text-dim my-2 bg-accent-subtle/25">{trimmed.slice(2)}</blockquote>;
      }
      return <p key={idx} className="text-xs text-text-muted leading-relaxed font-sans font-medium min-h-[1rem] mb-2">{trimmed}</p>;
    });
  };

  const renderCSV = (text: string) => {
    const rows = text.split("\n").map(r => r.split(","));
    const headers = rows[0] || [];
    const bodyRows = rows.slice(1).filter(r => r.some(cell => cell.trim().length > 0));
    return (
      <div className="overflow-x-auto border border-border-default rounded-xl max-h-[380px] bg-bg-surface">
        <table className="min-w-full divide-y divide-border-default text-[11px] text-left">
          <caption className="text-[9px] text-text-dim p-2 text-center bg-bg-surface-raised border-b border-border-default font-semibold uppercase tracking-wider font-mono">
            Simple CSV preview — complex formats may not render correctly
          </caption>
          <thead className="bg-bg-surface-raised sticky top-0 font-semibold text-text-primary">
            <tr>
              {headers.map((h, i) => (
                <th key={i} className="px-3 py-2 border-b border-border-default">{h.trim()}</th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-border-default text-text-muted">
            {bodyRows.map((row, rIdx) => (
              <tr key={rIdx} className="hover:bg-bg-base/30">
                {row.map((cell, cIdx) => (
                  <td key={cIdx} className="px-3 py-2 border-b border-border-default/60 truncate max-w-[150px]">{cell.trim()}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  };

  const renderPipeline = (status?: string) => {
    if (!status) return null;

    const stages = [
      { label: "Upload", key: "upload" },
      { label: "OCR", key: "ocr" },
      { label: "Chunk", key: "chunk" },
      { label: "Embed", key: "embed" },
      { label: "Indexed", key: "indexed" },
    ];

    let activeIndex = 0;
    let isFailed = status === "failed";

    if (status === "pending") {
      activeIndex = 1;
    } else if (status === "processing") {
      activeIndex = 3;
    } else if (status === "completed") {
      activeIndex = 5;
    }

    return (
      <div className="border border-border-default rounded-xl bg-bg-surface/50 p-4 space-y-3">
        <div className="flex items-center justify-between">
          <span className="text-[10px] uppercase font-bold tracking-wider text-text-muted">Extraction Pipeline</span>
          {isFailed && (
            <span className="text-[10px] font-semibold text-state-error uppercase tracking-wider flex items-center gap-1">
              <span className="h-1.5 w-1.5 rounded-full bg-state-error animate-pulse" />
              Failed
            </span>
          )}
          {status === "processing" && (
            <span className="text-[10px] font-semibold text-accent-primary uppercase tracking-wider flex items-center gap-1">
              <span className="h-1.5 w-1.5 rounded-full bg-accent-primary animate-pulse" />
              Processing
            </span>
          )}
          {status === "pending" && (
            <span className="text-[10px] font-semibold text-amber-500 uppercase tracking-wider flex items-center gap-1">
              <span className="h-1.5 w-1.5 rounded-full bg-amber-500 animate-pulse" />
              Queued
            </span>
          )}
          {status === "completed" && (
            <span className="text-[10px] font-semibold text-emerald-500 uppercase tracking-wider flex items-center gap-1">
              <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
              Indexed
            </span>
          )}
        </div>

        <div className="flex items-center justify-between relative mt-2 px-1">
          {stages.map((stage, idx) => {
            const isCompleted = !isFailed && idx < activeIndex;
            const isActive = !isFailed && idx === activeIndex;

            return (
              <React.Fragment key={stage.key}>
                {idx > 0 && (
                  <div
                    className={`flex-1 h-0.5 mx-1 transition-colors duration-500 ${
                      idx <= activeIndex && !isFailed
                        ? "bg-accent-primary"
                        : isFailed && idx <= activeIndex
                        ? "bg-state-error/30"
                        : "bg-border-default"
                    }`}
                  />
                )}
                <div className="flex flex-col items-center relative">
                  <div
                    className={`h-5 w-5 rounded-full flex items-center justify-center border text-[9px] font-semibold transition-all duration-300 ${
                      isCompleted
                        ? "bg-accent-primary border-accent-primary text-white"
                        : isActive
                        ? "border-accent-primary bg-bg-surface text-accent-primary animate-pulse"
                        : isFailed && idx === activeIndex
                        ? "bg-state-error border-state-error text-white font-bold"
                        : "border-border-default bg-bg-surface text-text-dim"
                    }`}
                  >
                    {isCompleted ? "✓" : isFailed && idx === activeIndex ? "✗" : idx + 1}
                  </div>
                  <span
                    className={`text-[9px] mt-1 font-semibold uppercase tracking-wider ${
                      isCompleted
                        ? "text-text-primary"
                        : isActive
                        ? "text-accent-primary font-bold"
                        : isFailed && idx === activeIndex
                        ? "text-state-error font-bold"
                        : "text-text-dim"
                    }`}
                  >
                    {stage.label}
                  </span>
                </div>
              </React.Fragment>
            );
          })}
        </div>

        {isFailed && (
          <div className="pt-2 border-t border-border-default/50 flex items-center justify-between gap-3">
            <p className="text-[11px] text-text-muted leading-relaxed">
              Something went wrong during extraction. You can retry the ingestion process.
            </p>
            <button
              onClick={() => reindexFile()}
              disabled={isReindexing}
              className="shrink-0 px-2.5 py-1 text-[10px] font-bold uppercase tracking-wider border border-state-error text-state-error hover:bg-state-error hover:text-white rounded-lg transition-colors disabled:opacity-50 cursor-pointer"
            >
              {isReindexing ? "Retrying..." : "Re-index"}
            </button>
          </div>
        )}
      </div>
    );
  };

  // Mutation to update tags list
  const { mutate: updateTags } = useMutation({
    mutationFn: async (tags: string[]) => {
      await api.put(`/api/v1/files/${fileId}/tags`, { tags });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["extraction", fileId] });
      queryClient.invalidateQueries({ queryKey: ["files"] });
      setNewTagValue("");
      setNewTagInputVisible(false);
    },
  });

  // Record file open access for recently opened widgets
  React.useEffect(() => {
    if (isOpen && fileId) {
      api.patch(`/api/v1/files/${fileId}/open`, {})
        .then(() => {
          queryClient.invalidateQueries({ queryKey: ["files"] });
        })
        .catch((err) => {
          console.error("Failed to record file access:", err);
        });
    }
  }, [isOpen, fileId, queryClient]);

  // Mutation to reindex file extraction
  const { mutate: reindexFile, isPending: isReindexing } = useMutation({
    mutationFn: async () => {
      await api.post(`/api/v1/files/${fileId}/reindex`, {});
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["extraction", fileId] });
      queryClient.invalidateQueries({ queryKey: ["files"] });
    },
  });

  const handleCopy = async () => {
    if (!data?.data.extractedText) return;
    try {
      await navigator.clipboard.writeText(data.data.extractedText);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error("Failed to copy to clipboard:", err);
    }
  };

  const handleExportMarkdown = () => {
    const text = data?.data.extractedText || fileContentText || "";
    if (!text) {
      toast.error("No text content available to export.");
      return;
    }
    const content = `# ${data?.data.originalName || "Exported Document"}\n\n${text}`;
    const blob = new Blob([content], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${data?.data.originalName?.replace(/\.[^/.]+$/, "") || "export"}.md`;
    a.click();
    URL.revokeObjectURL(url);
    toast.success("Markdown exported successfully.");
  };

  const handleAddTag = (e: React.FormEvent) => {
    e.preventDefault();
    const tag = newTagValue.trim();
    if (!tag) return;

    const currentTags = data?.data.tags || [];
    if (currentTags.includes(tag)) {
      setNewTagInputVisible(false);
      setNewTagValue("");
      return;
    }

    updateTags([...currentTags, tag]);
  };

  const handleRemoveTag = (tagToRemove: string) => {
    const currentTags = data?.data.tags || [];
    updateTags(currentTags.filter((t) => t !== tagToRemove));
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-40 flex justify-end select-none">
      {/* Backdrop overlay */}
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm transition-opacity"
        onClick={onClose}
      />

      {/* Slide-in container */}
      <div className="relative bg-bg-surface border-l border-border-default w-full max-w-[480px] h-screen shadow-2xl flex flex-col justify-between z-50 transform translate-x-0 transition-transform duration-300 animate-slide-in">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-border-default shrink-0">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <button
                onClick={onClose}
                className="p-1 rounded hover:bg-bg-base/60 text-text-muted hover:text-text-primary transition-colors cursor-pointer"
              >
                <ChevronRight className="h-4.5 w-4.5" />
              </button>
              <h3 className="font-serif text-base font-semibold text-text-primary truncate selection:bg-accent-primary/20 select-text" title={fileName}>
                {fileName}
              </h3>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg border border-border-default hover:border-border-strong bg-bg-base/50 text-text-muted hover:text-text-primary transition-all cursor-pointer"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Content body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6 bg-bg-base/20 select-none">
          {isLoading ? (
            <div className="flex flex-col items-center justify-center h-full space-y-4">
              <Loader2 className="h-8 w-8 text-accent-primary animate-spin" />
              <div className="text-xs text-text-muted uppercase tracking-wider animate-pulse font-mono">
                Loading file...
              </div>
            </div>
          ) : error ? (
            <div className="flex flex-col items-center justify-center h-full text-center max-w-sm mx-auto space-y-4">
              <div className="h-12 w-12 rounded-full bg-state-error/10 border border-state-error/25 flex items-center justify-center">
                <AlertCircle className="h-6 w-6 text-state-error" />
              </div>
              <div>
                <h4 className="font-serif text-base text-text-primary">Failed to load file</h4>
                <p className="text-xs text-text-muted mt-2 leading-relaxed">
                  {(error as ApiError).message || "Failed to load document content."}
                </p>
              </div>
            </div>
          ) : (
            <div className="space-y-6">
              {/* File Info pill row */}
              <div className="flex items-center gap-2">
                {getFileTypeBadge(data?.data.mimeType || "")}
                <span className="px-2 py-0.5 rounded-full text-[10px] font-mono bg-bg-surface-raised border border-border-default text-text-muted">
                  {formatBytes(data?.data.sizeBytes || 0)}
                </span>
              </div>

              {/* Collection Folder Picker */}
              <div className="space-y-2 border border-border-default rounded-xl bg-bg-surface/30 p-4 shadow-sm select-none">
                <h4 className="text-[10px] uppercase font-bold tracking-wider text-text-muted flex items-center gap-1.5 font-mono">
                  <Folder className="h-3.5 w-3.5 text-accent-primary shrink-0" />
                  Collection Folder
                </h4>
                <select
                  value={data?.data.collectionId || ""}
                  onChange={(e) => {
                    const val = e.target.value;
                    updateCollection(val === "" ? null : val);
                  }}
                  className="w-full bg-bg-surface border border-border-default hover:border-border-strong text-text-primary text-xs font-semibold px-3 py-2 rounded-xl focus:outline-none focus:border-accent-primary cursor-pointer transition-all duration-[120ms] ease-motion"
                >
                  <option value="">None (Unassigned)</option>
                  {collectionsList.map((col) => (
                    <option key={col.id} value={col.id}>
                      {col.name}
                    </option>
                  ))}
                </select>
              </div>

              {/* Extraction Ingestion Lifecycle Pipeline */}
              {renderPipeline(data?.data.status)}

              {/* Collapsible AI Summary Section */}
              <div className="border border-border-default rounded-xl bg-bg-surface/50 overflow-hidden shadow-sm">
                <button
                  onClick={() => setSummaryExpanded(!summaryExpanded)}
                  className="w-full flex items-center justify-between p-4 hover:bg-bg-surface-hover/30 transition-colors border-b border-border-default/60"
                >
                  <div className="flex items-center gap-2">
                    <Brain className="h-4 w-4 text-accent-primary" />
                    <span className="text-xs font-semibold text-text-primary">AI Summary</span>
                  </div>
                  <ChevronRight
                    className={`h-4 w-4 text-text-dim transition-transform ${summaryExpanded ? "transform rotate-90" : ""}`}
                  />
                </button>
                {summaryExpanded && (
                  <div className="p-4 text-xs leading-relaxed text-text-muted selection:bg-accent-primary/20 select-text animate-fade-in">
                    {data?.data.summary || "No automated summary synthesized for this file. Summaries are indexed during worker queues."}
                  </div>
                )}
              </div>

              {/* Dynamic Tag Management */}
              <div className="space-y-2">
                <h4 className="text-[10px] uppercase font-bold tracking-wider text-text-muted flex items-center gap-1.5">
                  <Tag className="h-3 w-3 text-accent-primary" />
                  Tags
                </h4>
                <div className="flex flex-wrap items-center gap-1.5 bg-bg-surface/30 border border-border-default rounded-xl p-3 min-h-[48px]">
                  {data?.data.tags?.map((tag) => (
                    <span
                      key={tag}
                      className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[10px] font-semibold bg-bg-surface-raised border border-border-default text-text-muted"
                    >
                      {tag}
                      <button
                        onClick={() => handleRemoveTag(tag)}
                        className="text-text-dim hover:text-state-error transition-colors cursor-pointer text-xs"
                        title={`Remove "${tag}"`}
                      >
                        &times;
                      </button>
                    </span>
                  ))}

                  {/* Inline Tag Adding form */}
                  {newTagInputVisible ? (
                    <form onSubmit={handleAddTag} className="flex items-center gap-1.5">
                      <input
                        type="text"
                        autoFocus
                        value={newTagValue}
                        onChange={(e) => setNewTagValue(e.target.value)}
                        onBlur={() => setNewTagInputVisible(false)}
                        placeholder="Tag name..."
                        className="bg-bg-base border border-accent-primary/40 px-2 py-0.5 rounded-full text-[10px] text-text-primary placeholder:text-text-dim focus:outline-none focus:border-accent-primary min-w-[70px] max-w-[100px]"
                      />
                    </form>
                  ) : (
                    <button
                      onClick={() => setNewTagInputVisible(true)}
                      className="inline-flex items-center gap-1 px-2.5 py-0.5 border border-dashed border-border-default hover:border-accent-primary rounded-full text-[10px] font-semibold text-text-dim hover:text-accent-primary bg-transparent transition-all cursor-pointer"
                    >
                      <Plus className="h-3 w-3" />
                      Add Tag
                    </button>
                  )}

                  {(!data?.data.tags || data.data.tags.length === 0) && !newTagInputVisible && (
                    <span className="text-[10px] text-text-dim italic py-0.5 pl-1 select-none">No tags defined yet.</span>
                  )}
                </div>
              </div>

              {/* OCR Statistics */}
              {data?.data.meta && (
                <div className="space-y-2">
                  <h4 className="text-[10px] uppercase font-bold tracking-wider text-text-muted">File info</h4>
                  <div className="grid grid-cols-2 gap-4 p-4 rounded-xl border border-border-default bg-bg-surface/30 text-[10px] font-semibold tracking-wider font-mono">
                    <div>
                      <span className="text-text-dim block uppercase">Engine</span>
                      <span className="text-text-primary mt-1 block font-sans truncate">{data.data.meta.engine || "Native Parser"}</span>
                    </div>
                    <div>
                      <span className="text-text-dim block uppercase">Word Count</span>
                      <span className="text-text-primary mt-1 block">{data.data.meta.wordCount || 0} words</span>
                    </div>
                    <div>
                      <span className="text-text-dim block uppercase">Pages</span>
                      <span className="text-text-primary mt-1 block">{data.data.meta.pages || 1} pages</span>
                    </div>
                    <div>
                      <span className="text-text-dim block uppercase">Extraction time</span>
                      <span className="text-text-primary mt-1 block">
                        {data.data.meta.durationMs ? `${(data.data.meta.durationMs / 1000).toFixed(2)}s` : "n/a"}
                      </span>
                    </div>
                  </div>
                </div>
              )}

              {/* Similar/Related Files Section */}
              {similarList.length > 0 && (
                <div className="space-y-2 select-none animate-fade-in">
                  <h4 className="text-[10px] uppercase font-bold tracking-wider text-text-muted flex items-center gap-1.5">
                    <Brain className="h-3 w-3 text-accent-primary" />
                    Similar Files (Semantic Similarity)
                  </h4>
                  <div className="flex flex-col gap-2 p-3 bg-bg-surface/30 border border-border-default rounded-xl">
                    {similarList.map((simFile) => (
                      <div
                        key={simFile.id}
                        onClick={() => {
                          setSelectedFile(simFile.id, simFile.originalName);
                        }}
                        className="flex items-center justify-between p-2 rounded-lg bg-bg-base/30 hover:bg-bg-surface-raised transition-colors border border-border-default/50 group cursor-pointer"
                      >
                        <div className="flex items-center gap-2.5 min-w-0">
                          {getFileTypeIcon(simFile.mimeType)}
                          <span className="text-[11px] font-sans font-medium text-text-primary truncate max-w-[200px]" title={simFile.originalName}>
                            {simFile.originalName}
                          </span>
                        </div>
                        <span className="text-[9px] font-mono font-semibold bg-accent-subtle border border-accent-subtle-border px-1.5 py-0.5 rounded text-accent-primary shrink-0 select-none">
                          {Math.round(simFile.similarity * 100)}% Match
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Unified Source Viewer Selector */}
              <div className="space-y-2 flex-1 flex flex-col min-h-[300px]">
                <h4 className="text-[10px] uppercase font-bold tracking-wider text-text-muted">Source Viewer</h4>
                {mediaLoading ? (
                  <div className="flex flex-col items-center justify-center py-16 border border-border-default rounded-xl bg-bg-surface/30 space-y-3">
                    <Loader2 className="h-6 w-6 text-accent-primary animate-spin" />
                    <span className="text-[10px] text-text-muted uppercase tracking-wider animate-pulse font-mono">Stream loading...</span>
                  </div>
                ) : mediaError ? (
                  <div className="flex items-center gap-3 p-4 border border-state-error/25 bg-state-error/10 text-state-error text-xs rounded-xl">
                    <AlertCircle className="h-4.5 w-4.5 shrink-0" />
                    <span>{mediaError}</span>
                  </div>
                ) : (
                  <div className="flex-1 flex flex-col">
                    {/* Render format-specific components */}
                    {!isSupportedFormat && !forceTextView && (
                      <div className="bg-bg-surface border border-border-default rounded-xl p-6 flex flex-col items-center text-center space-y-4">
                        <div className="h-12 w-12 rounded-full bg-state-error/10 border border-state-error/20 flex items-center justify-center text-state-error shrink-0 animate-pulse">
                          <AlertCircle className="h-6 w-6" />
                        </div>
                        <div className="space-y-2 max-w-sm">
                          <h5 className="text-sm font-semibold text-text-primary">Unsupported File Format</h5>
                          <p className="text-xs text-text-muted leading-relaxed">
                            The format <code className="bg-bg-base px-1.5 py-0.5 rounded text-[11px] font-mono text-text-primary border border-border-default">{mimeType || "unknown"}</code> is not natively supported for inline previewing.
                          </p>
                          <p className="text-[11px] text-text-dim leading-relaxed">
                            AKASHA has successfully uploaded, cataloged, and indexed this file by its properties and metadata, keeping it semantically searchable by name and description.
                          </p>
                        </div>
                        <div className="w-full border-t border-border-default pt-4 flex flex-col gap-2.5">
                          <p className="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-1">Available Actions</p>
                          <div className="flex gap-2 justify-center flex-wrap">
                            <button
                              onClick={onDownload}
                              disabled={isDownloading}
                              className="px-3 py-1.5 bg-accent-primary hover:bg-accent-primary-hover disabled:bg-border-default text-accent-primary-foreground font-semibold rounded-lg text-xs flex items-center gap-1.5 transition-colors border border-accent-primary"
                            >
                              {isDownloading ? <Loader2 className="h-3 w-3 animate-spin" /> : <Download className="h-3 w-3" />}
                              Download File
                            </button>
                            <button
                              onClick={onDelete}
                              disabled={isDeleting}
                              className="px-3 py-1.5 bg-state-error/10 hover:bg-state-error/25 disabled:bg-border-default text-state-error font-semibold rounded-lg text-xs flex items-center gap-1.5 transition-colors border border-state-error/20"
                            >
                              {isDeleting ? <Loader2 className="h-3 w-3 animate-spin" /> : <Trash2 className="h-3 w-3" />}
                              Delete File
                            </button>
                            <button
                              onClick={() => setForceTextView(true)}
                              className="px-3 py-1.5 bg-bg-base hover:bg-bg-surface-raised border border-border-default text-text-primary hover:text-text-primary font-semibold rounded-lg text-xs flex items-center gap-1.5 transition-colors"
                            >
                              Force View Text
                            </button>
                          </div>
                        </div>
                      </div>
                    )}

                    {forceTextView && fileContentText && (
                      <div className="flex-1 flex flex-col space-y-2">
                        <div className="flex items-center justify-between">
                          <span className="text-[10px] uppercase font-bold tracking-wider text-text-muted">Raw Text Content</span>
                          <button
                            onClick={() => setForceTextView(false)}
                            className="text-[10px] text-accent-primary hover:text-accent-primary-hover font-semibold transition-colors"
                          >
                            ← Back to Option Selector
                          </button>
                        </div>
                        <pre className="font-mono text-[11px] leading-relaxed text-text-primary bg-bg-surface border border-border-default rounded-xl p-4 overflow-x-auto max-h-[380px] select-text selection:bg-accent-primary/20 whitespace-pre-wrap">
                          <code>{fileContentText}</code>
                        </pre>
                      </div>
                    )}

                    {isSupportedFormat && data?.data.mimeType?.includes("pdf") && blobUrl && (
                      <iframe src={`${blobUrl}#toolbar=0`} className="w-full h-[380px] border border-border-default rounded-xl bg-black" />
                    )}
                    
                    {isSupportedFormat && data?.data.mimeType?.includes("image") && blobUrl && (
                      <div className="relative overflow-hidden border border-border-default rounded-xl bg-bg-surface-raised flex items-center justify-center p-4">
                        <img
                          src={blobUrl}
                          onClick={() => window.open(blobUrl, "_blank")}
                          className="max-h-[380px] object-contain transition-transform duration-200 hover:scale-105 cursor-zoom-in"
                          title="Click to view full size"
                        />
                      </div>
                    )}
                    
                    {isSupportedFormat && (data?.data.mimeType?.includes("markdown") || data?.data.mimeType?.includes("x-markdown")) && fileContentText && (
                      <div className="flex-1 bg-bg-surface border border-border-default rounded-xl p-4 overflow-y-auto max-h-[380px] select-text selection:bg-accent-primary/20">
                        {renderMarkdown(fileContentText)}
                      </div>
                    )}
                    
                    {isSupportedFormat && data?.data.mimeType?.includes("csv") && fileContentText && (
                      renderCSV(fileContentText)
                    )}
                    
                    {isSupportedFormat && data?.data.mimeType?.includes("text/plain") && !forceTextView && fileContentText && (
                      <pre className="font-mono text-[11px] leading-relaxed text-text-primary bg-bg-surface border border-border-default rounded-xl p-4 overflow-x-auto max-h-[380px] select-text selection:bg-accent-primary/20 whitespace-pre-wrap">
                        <code>{fileContentText}</code>
                      </pre>
                    )}
                    
                    {isSupportedFormat && data?.data.mimeType?.includes("video") && blobUrl && (
                      <div className="bg-black border border-border-default rounded-xl overflow-hidden flex items-center justify-center p-1">
                        <video src={blobUrl} controls className="w-full max-h-[350px]" />
                      </div>
                    )}
                    
                    {isSupportedFormat && data?.data.mimeType?.includes("audio") && blobUrl && (
                      <div className="bg-bg-surface border border-border-default rounded-xl p-4 flex flex-col gap-3">
                        <div className="flex items-center gap-3">
                          <div className="h-10 w-10 bg-amber-400/10 border border-amber-400/20 text-amber-400 rounded-lg flex items-center justify-center shrink-0">
                            <Play className="h-5 w-5 fill-amber-400" />
                          </div>
                          <div className="min-w-0 flex-1">
                            <p className="text-xs font-semibold text-text-primary truncate">{fileName}</p>
                            <p className="text-[10px] text-text-dim uppercase tracking-wider mt-0.5 font-mono">Audio Track</p>
                          </div>
                        </div>
                        <audio src={blobUrl} controls className="w-full mt-1 [&::-webkit-media-controls-panel]:bg-bg-surface" />
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-6 py-4 border-t border-border-default bg-bg-surface flex items-center justify-between shrink-0">
          <button
            onClick={onDelete}
            disabled={isDeleting || isDownloading}
            className="flex items-center gap-1.5 px-3 py-2 border border-transparent hover:bg-state-error/10 hover:border-state-error/25 text-state-error rounded-lg text-xs font-semibold uppercase tracking-wider transition-all disabled:opacity-50 cursor-pointer"
          >
            {isDeleting ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Trash2 className="h-3.5 w-3.5" />}
            Delete file
          </button>

          <div className="flex items-center gap-3">
            {data?.data.status === "failed" && (
              <button
                onClick={() => reindexFile()}
                disabled={isReindexing}
                className="flex items-center gap-1.5 px-4 py-2 border border-state-error hover:bg-state-error/10 text-state-error rounded-lg text-xs font-semibold uppercase tracking-wider transition-all disabled:opacity-50 cursor-pointer"
              >
                {isReindexing ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ChevronRight className="h-3.5 w-3.5 text-state-error" />}
                Re-index
              </button>
            )}
            <button
              onClick={handleCopy}
              disabled={isLoading || !!error}
              className="flex items-center gap-1.5 px-4 py-2 border border-border-default hover:border-border-strong bg-bg-base hover:bg-bg-surface-raised rounded-lg text-xs font-semibold uppercase tracking-wider text-text-primary transition-all disabled:opacity-50 cursor-pointer"
            >
              {copied ? (
                <>
                  <Check className="h-3.5 w-3.5 text-state-success" />
                  Copied
                </>
              ) : (
                <>
                  <Copy className="h-3.5 w-3.5 text-text-muted" />
                  Copy Text
                </>
              )}
            </button>

            <button
              onClick={handleExportMarkdown}
              disabled={isLoading || !!error || (!data?.data.extractedText && !fileContentText)}
              className="flex items-center gap-1.5 px-4 py-2 border border-border-default hover:border-border-strong bg-bg-base hover:bg-bg-surface-raised rounded-lg text-xs font-semibold uppercase tracking-wider text-text-primary transition-all disabled:opacity-50 cursor-pointer"
            >
              <FileDown className="h-3.5 w-3.5 text-text-muted" />
              Export Markdown
            </button>

            <button
              onClick={onDownload}
              disabled={isLoading || !!error || isDownloading}
              className="flex items-center gap-1.5 px-4 py-2 bg-accent-primary hover:bg-accent-hover text-text-primary rounded-lg text-xs font-semibold uppercase tracking-wider transition-all disabled:opacity-50 cursor-pointer shadow-md"
            >
              {isDownloading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Download className="h-3.5 w-3.5" />}
              Download
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
