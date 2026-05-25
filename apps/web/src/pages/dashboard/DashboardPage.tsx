import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { useAuthStore } from "../../store/auth.store";
import { useUIStore } from "../../store/ui.store";
import { useState, useEffect, useRef } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { AlertCircle, Star, Download, Trash2, LayoutGrid, List, Check, Pin, Loader2, Zap, ArrowRight } from "lucide-react";
import { FileCard, FileItem, formatBytes, getFileTypeIcon, getStatusBadge } from "../../components/files/FileCard";
import { FileGrid } from "../../components/files/FileGrid";
import { FileDetailSheet } from "../../components/files/FileDetailSheet";
import { UploadZone } from "../../components/upload/UploadZone";
import { Dialog } from "../../components/ui/Dialog";
import { ErrorBoundary } from "../../components/ui/ErrorBoundary";
import { CanvasBackground } from "../../components/ui/CanvasBackground";
import { toast } from "sonner";

export function DashboardPage() {
  const { accessToken } = useAuthStore();
  const queryClient = useQueryClient();

  // Zustand UI store state handles selected file and uploads progress
  const {
    selectedFileId,
    selectedFileName,
    uploadProgress,
    setSelectedFile,
    activeUploads,
    addActiveUpload,
    updateActiveUpload,
    clearActiveUploads,
  } = useUIStore();

  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [isUploading, setIsUploading] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);

  // Starred toggle state tracking
  const [starringId, setStarringId] = useState<string | null>(null);
  // Global drag-over state tracking
  const [isDraggingGlobal, setIsDraggingGlobal] = useState(false);
  // Keyboard selector focus card state
  const [focusedCardId, setFocusedCardId] = useState<string | null>(null);
  // Track previous processing files count to trigger indexing toasts
  const [prevProcessingCount, setPrevProcessingCount] = useState(0);
  // Ref-based drag counter to prevent overlay leaf flickering
  const dragCounter = useRef(0);

  // Read search parameters for collections sidebar selection
  const [searchParams, setSearchParams] = useSearchParams();
  const collectionIdFilter = searchParams.get("collectionId");
  const openFileId = searchParams.get("openFileId");

  // Multi-select state for bulk operations
  const [selectedFileIds, setSelectedFileIds] = useState<string[]>([]);

  // Dashboard Richness filters state
  const [quickFilter, setQuickFilter] = useState("");
  const [selectedTagFilters, setSelectedTagFilters] = useState<string[]>([]);
  const [viewMode, setViewMode] = useState<"grid" | "list">(() => {
    return (localStorage.getItem("dashboard_view_mode") as "grid" | "list") || "grid";
  });

  // Sorting state (adds sorting support by status!)
  const [sortBy, setSortBy] = useState<"name" | "date" | "size" | "type" | "status">("date");
  const [sortOrder, setSortOrder] = useState<"asc" | "desc">("desc");

  // Pagination state
  const [currentPage, setCurrentPage] = useState(1);
  const itemsPerPage = 6;

  // Shortcuts overlay state
  const [shortcutsOpen, setShortcutsOpen] = useState(false);

  // States to control the general info/alert dialog
  const [infoDialog, setInfoDialog] = useState<{ open: boolean; title: string; description: string }>({
    open: false,
    title: "",
    description: "",
  });

  // States to control the styled delete confirm dialog
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [bulkDeleteConfirmOpen, setBulkDeleteConfirmOpen] = useState(false);
  const [fileIdToDelete, setFileIdToDelete] = useState<string | null>(null);
  const [fileNameToDelete, setFileNameToDelete] = useState("");

  // 1. Fetch file list with dynamic polling if any files are pending/processing
  const { data: filesData, isLoading, error: fetchError } = useQuery<{ data: FileItem[] }>({
    queryKey: ["files"],
    queryFn: () => api.get("/api/v1/files"),
    refetchInterval: (query) => {
      const filesList = query.state.data?.data;
      if (!filesList) return false;
      const hasActiveJobs = filesList.some(
        (file) => file.status === "pending" || file.status === "processing"
      );
      return hasActiveJobs ? 3000 : false;
    },
  });

  // 1.5 Fetch secure usage/stats data directly for Significa metrics widgets
  const { data: usageData } = useQuery<{ data: { fileCount: number; storageUsed: number; storageLimit: number; chunkCount: number } }>({
    queryKey: ["user-usage"],
    queryFn: () => api.get("/api/v1/user/usage"),
    enabled: !!accessToken,
  });

  const filesList = filesData?.data || [];
  const pinnedFiles = filesList.filter((f) => f.isPinned).slice(0, 3);
  const mainFiles = filesList.filter((f) => !f.isPinned);
  const regularFiles = mainFiles;
  const processingFiles = filesList.filter((f) => f.status === "pending" || f.status === "processing");
  const failedFiles = filesList.filter((f) => f.status === "failed");

  // Sync openFileId URL parameter dynamically after filesList is declared
  useEffect(() => {
    if (openFileId && filesList.length > 0) {
      const file = filesList.find((f) => f.id === openFileId);
      if (file) {
        setSelectedFile(file.id, file.originalName);
        const nextParams = new URLSearchParams(searchParams);
        nextParams.delete("openFileId");
        setSearchParams(nextParams, { replace: true });
      }
    }
  }, [openFileId, filesList, searchParams, setSearchParams, setSelectedFile]);

  // Fetch user collections dynamically for organizing files via context menu
  const { data: collectionsData } = useQuery<{ data: Array<{ id: string; name: string; color: string }> }>({
    queryKey: ["collections"],
    queryFn: () => api.get("/api/v1/collections"),
    enabled: !!accessToken,
  });
  const collectionsList = collectionsData?.data || [];

  const handleMoveFileToCollection = async (fileId: string, colId: string) => {
    try {
      await api.patch(`/api/v1/files/${fileId}`, { collectionId: colId });
      toast.success("File organized successfully.");
      queryClient.invalidateQueries({ queryKey: ["files"] });
    } catch (err: any) {
      toast.error(err.message || "Failed to organize file.");
    }
  };

  // "Continue where you left off" - 3 most recently opened files
  const recentFiles = [...filesList]
    .filter((f) => f.lastOpenedAt)
    .sort((a, b) => new Date(b.lastOpenedAt!).getTime() - new Date(a.lastOpenedAt!).getTime())
    .slice(0, 3);

  // Extract unique tags list
  const allTags = Array.from(
    new Set(
      filesList
        .flatMap((f) => f.tags || [])
        .filter(Boolean)
    )
  ).slice(0, 12);

  // Client-side quick filter logic (respects collection folders!)
  const matchesFilter = (file: FileItem) => {
    const matchesQuery = file.originalName.toLowerCase().includes(quickFilter.toLowerCase());
    const matchesTags = 
      selectedTagFilters.length === 0 || 
      selectedTagFilters.every(tag => file.tags?.includes(tag));
    const matchesCollection = !collectionIdFilter || file.collectionId === collectionIdFilter;
    return matchesQuery && matchesTags && matchesCollection;
  };

  const filteredRegularFiles = regularFiles.filter(matchesFilter);
  const usage = usageData?.data;

  // Rich Client-side Sorting (now with sorting by status!)
  const sortFiles = (filesToSort: FileItem[]) => {
    return [...filesToSort].sort((a, b) => {
      let comparison = 0;
      if (sortBy === "name") {
        comparison = a.originalName.localeCompare(b.originalName);
      } else if (sortBy === "date") {
        comparison = new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime();
      } else if (sortBy === "size") {
        comparison = a.sizeBytes - b.sizeBytes;
      } else if (sortBy === "type") {
        comparison = a.mimeType.localeCompare(b.mimeType);
      } else if (sortBy === "status") {
        comparison = a.status.localeCompare(b.status);
      }
      return sortOrder === "asc" ? comparison : -comparison;
    });
  };

  const sortedRegularFiles = sortFiles(filteredRegularFiles);

  // Pagination calculation
  const totalRegularItems = sortedRegularFiles.length;
  const totalRegularPages = Math.ceil(totalRegularItems / itemsPerPage);
  const paginatedRegularFiles = sortedRegularFiles.slice(
    (currentPage - 1) * itemsPerPage,
    currentPage * itemsPerPage
  );

  const toggleViewMode = () => {
    setViewMode((prev) => {
      const next = prev === "grid" ? "list" : "grid";
      localStorage.setItem("dashboard_view_mode", next);
      return next;
    });
  };

  // Reset pagination to first page when quick filter, tags, or sorting criteria changes
  useEffect(() => {
    setCurrentPage(1);
  }, [quickFilter, selectedTagFilters, sortBy, sortOrder, collectionIdFilter]);

  // Track indexing completion state transitions
  useEffect(() => {
    if (!filesList || filesList.length === 0) return;
    const currentProcessingCount = filesList.filter(
      (f) => f.status === "pending" || f.status === "processing"
    ).length;

    if (prevProcessingCount > 0 && currentProcessingCount === 0) {
      toast.success("Indexing completed. All files are now searchable.");
    }
    setPrevProcessingCount(currentProcessingCount);
  }, [filesList, prevProcessingCount]);

  // 2. Sequential Ingestion Queue
  const handleUploadMultiple = async (files: File[]) => {
    setIsUploading(true);
    setUploadError(null);
    clearActiveUploads();

    const uploadItems = files.map((file, index) => {
      const item = {
        id: `upload-${Date.now()}-${index}`,
        name: file.name,
        progress: 0,
        status: "uploading" as const,
      };
      addActiveUpload(item);
      return { file, item };
    });

    try {
      for (const { file, item } of uploadItems) {
        const formData = new FormData();
        formData.append("file", file);

        try {
          await api.uploadWithProgress("/api/v1/files", formData, (percent) => {
            updateActiveUpload(item.id, { progress: percent });
          });
          updateActiveUpload(item.id, { progress: 100, status: "completed" });
        } catch (err: any) {
          updateActiveUpload(item.id, { 
            status: "failed", 
            error: err.message || "Failed to upload." 
          });
        }
      }
      queryClient.invalidateQueries({ queryKey: ["files"] });
    } catch (err: any) {
      setUploadError(err.message || "Failed to upload files.");
    } finally {
      setIsUploading(false);
    }
  };

  // 3. File Deletion Mutation
  const { mutate: deleteFile, isPending: isDeleting } = useMutation({
    mutationFn: async (fileId: string) => {
      await api.delete(`/api/v1/files/${fileId}`);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["files"] });
      setDeleteConfirmOpen(false);
      setFileIdToDelete(null);
      setFileNameToDelete("");
      toast.success("File deleted.");

      // If we deleted the active opened file, close panel
      if (selectedFileId && selectedFileId === fileIdToDelete) {
        setSelectedFile(null);
      }
    },
    onError: (err: any) => {
      setInfoDialog({
        open: true,
        title: "Delete failed",
        description: err.message || "Failed to delete metadata record.",
      });
      toast.error(`Deletion failed: ${err.message || "An error occurred."}`);
      setDeleteConfirmOpen(false);
    },
  });

  // 3.6 Bulk file deletion mutation
  const { mutate: bulkDelete } = useMutation({
    mutationFn: async (fileIds: string[]) => {
      await api.post("/api/v1/files/bulk-delete", { fileIds });
    },
    onSuccess: (_, fileIds) => {
      queryClient.invalidateQueries({ queryKey: ["files"] });
      setSelectedFileIds([]);
      toast.success(`Successfully deleted ${fileIds.length} files.`);
    },
    onError: (err: any) => {
      toast.error(`Bulk deletion failed: ${err.message || "An error occurred."}`);
    },
  });

  // 3.5 Star / Favorite Mutation
  const { mutate: toggleStar } = useMutation({
    mutationFn: async ({ fileId, isStarred }: { fileId: string; isStarred: boolean }) => {
      setStarringId(fileId);
      await api.patch(`/api/v1/files/${fileId}`, { isStarred });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["files"] });
    },
    onSettled: () => {
      setStarringId(null);
    },
  });

  // 3.8 Pin / Unpin Mutation
  const { mutate: togglePin } = useMutation({
    mutationFn: async ({ fileId, isPinned }: { fileId: string; isPinned: boolean }) => {
      await api.patch(`/api/v1/files/${fileId}`, { isPinned });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["files"] });
      toast.success("Archive structure updated.");
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to update pinned state.");
    },
  });

  // Quick Capture Mutation
  const [captureInput, setCaptureInput] = useState("");
  const { mutate: captureThought, isPending: isCapturing } = useMutation({
    mutationFn: async (content: string) => {
      const isUrl = content.startsWith("http://") || content.startsWith("https://");
      return api.post("/api/v1/files/capture", {
        content,
        type: isUrl ? "url" : "note"
      });
    },
    onSuccess: () => {
      setCaptureInput("");
      queryClient.invalidateQueries({ queryKey: ["files"] });
      queryClient.invalidateQueries({ queryKey: ["user-usage"] });
      toast.success("Thought captured. Ingestion started.");
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to capture thought.");
    }
  });

  const handleCaptureSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = captureInput.trim();
    if (!trimmed || isCapturing) return;
    captureThought(trimmed);
  };

  // Global drag and drop window tracking
  useEffect(() => {
    const handleDragEnter = (e: DragEvent) => {
      e.preventDefault();
      if (e.dataTransfer?.types.includes("Files")) {
        dragCounter.current++;
        setIsDraggingGlobal(true);
      }
    };

    const handleDragOver = (e: DragEvent) => {
      e.preventDefault();
    };

    const handleDragLeave = (e: DragEvent) => {
      e.preventDefault();
      dragCounter.current--;
      if (dragCounter.current <= 0) {
        setIsDraggingGlobal(false);
        dragCounter.current = 0;
      }
    };

    const handleDrop = (e: DragEvent) => {
      e.preventDefault();
      dragCounter.current = 0;
      setIsDraggingGlobal(false);
      const files = e.dataTransfer?.files;
      if (files && files.length > 0) {
        handleUploadMultiple(Array.from(files));
      }
    };

    window.addEventListener("dragenter", handleDragEnter);
    window.addEventListener("dragover", handleDragOver);
    window.addEventListener("dragleave", handleDragLeave);
    window.addEventListener("drop", handleDrop);

    return () => {
      window.removeEventListener("dragenter", handleDragEnter);
      window.removeEventListener("dragover", handleDragOver);
      window.removeEventListener("dragleave", handleDragLeave);
      window.removeEventListener("drop", handleDrop);
    };
  }, [isUploading]);

  const navigate = useNavigate();

  // / and ? Key navigating to Search Page / Shortcuts help panel
  useEffect(() => {
    const handleGlobalKey = (e: KeyboardEvent) => {
      if (
        document.activeElement?.tagName === "INPUT" ||
        document.activeElement?.tagName === "TEXTAREA"
      ) {
        return;
      }
      if (e.key === "/") {
        e.preventDefault();
        navigate("/search?focus=true");
      } else if (e.key === "?") {
        e.preventDefault();
        setShortcutsOpen((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handleGlobalKey);
    return () => window.removeEventListener("keydown", handleGlobalKey);
  }, [navigate]);

  // j/k selector keys handler
  useEffect(() => {
    const handleGlobalKey = (e: KeyboardEvent) => {
      if (
        document.activeElement?.tagName === "INPUT" ||
        document.activeElement?.tagName === "TEXTAREA" ||
        isDeleting ||
        isUploading ||
        deleteConfirmOpen
      ) {
        return;
      }

      if (filesList.length === 0) return;

      // Select All shortcut (⌘A or Ctrl+A)
      if ((e.metaKey || e.ctrlKey) && e.key === "a") {
        e.preventDefault();
        const pageFileIds = paginatedRegularFiles.map((f) => f.id);
        setSelectedFileIds((prev) => {
          const allSelected = pageFileIds.every((id) => prev.includes(id));
          if (allSelected) {
            return prev.filter((id) => !pageFileIds.includes(id));
          } else {
            return Array.from(new Set([...prev, ...pageFileIds]));
          }
        });
        return;
      }

      const currentIndex = filesList.findIndex((f) => f.id === focusedCardId);

      if (e.key === "j" || e.key === "ArrowDown") {
        e.preventDefault();
        const nextIndex = (currentIndex + 1) % filesList.length;
        setFocusedCardId(filesList[nextIndex].id);
      } else if (e.key === "k" || e.key === "ArrowUp") {
        e.preventDefault();
        const prevIndex = (currentIndex - 1 + filesList.length) % filesList.length;
        setFocusedCardId(filesList[prevIndex].id);
      } else if (e.key === "Enter") {
        if (focusedCardId) {
          e.preventDefault();
          setSelectedFile(focusedCardId, filesList.find(f => f.id === focusedCardId)?.originalName || "");
        }
      } else if (e.key === "d") {
        if (focusedCardId) {
          e.preventDefault();
          const file = filesList.find((f) => f.id === focusedCardId);
          if (file) handleDownload(file);
        }
      } else if (e.key === "Delete" || e.key === "Backspace") {
        if (focusedCardId) {
          e.preventDefault();
          const file = filesList.find((f) => f.id === focusedCardId);
          if (file) triggerDeleteConfirm(file);
        }
      } else if (e.key === "Escape" && selectedFileId) {
        e.preventDefault();
        setSelectedFile(null);
      }
    };

    window.addEventListener("keydown", handleGlobalKey);
    return () => window.removeEventListener("keydown", handleGlobalKey);
  }, [focusedCardId, filesList, isDeleting, isUploading, deleteConfirmOpen, selectedFileId, setSelectedFile]);

  // 4. Secure Binary Download Stream
  const handleDownload = async (file: FileItem) => {
    if (downloadingId) return;
    setDownloadingId(file.id);

    try {
      const response = await fetch(`/api/v1/files/${file.id}/download`, {
        headers: {
          Authorization: `Bearer ${accessToken}`,
        },
      });

      if (!response.ok) {
        throw new Error("The file could not be downloaded. It may have been removed from storage.");
      }

      const blob = await response.blob();
      const url = window.URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.setAttribute("download", file.originalName);
      document.body.appendChild(link);
      link.click();
      link.remove();
      window.URL.revokeObjectURL(url);
    } catch (err: any) {
      setInfoDialog({
        open: true,
        title: "Download failed",
        description: "The file could not be downloaded. It may have been moved or deleted from storage.",
      });
    } finally {
      setDownloadingId(null);
    }
  };

  const triggerDeleteConfirm = (file: FileItem) => {
    setFileIdToDelete(file.id);
    setFileNameToDelete(file.originalName);
    setDeleteConfirmOpen(true);
  };

  const executeDeletion = () => {
    if (fileIdToDelete) {
      deleteFile(fileIdToDelete);
    }
  };

  return (
    <ErrorBoundary>
      <div className="space-y-12 select-none relative">
        {/* Canvas Background Sensitivity on Empty Dashboard States */}
        {!isLoading && filesList.length === 0 && (
          <CanvasBackground intensity="barely" />
        )}

        {/* Global Full-Window Drag Zone Backdrop Shimmer Overlay */}
        {isDraggingGlobal && (
          <div className="fixed inset-0 bg-bg-base/80 backdrop-blur-md border-4 border-dashed border-accent-primary z-50 flex flex-col justify-center items-center p-8 animate-fade-in select-none">
            <div className="max-w-md text-center space-y-4">
              <h2 className="font-serif text-3xl font-normal text-text-primary leading-tight">
                Drop files anywhere to upload.
              </h2>
              <p className="text-xs text-text-muted leading-relaxed font-sans font-medium">
                AKASHA will index and make your files searchable.
              </p>
            </div>
          </div>
        )}

        {/* Page title and eyebrow heading */}
        <div className="border-b border-border-default pb-6 flex items-center justify-between">
          <div>
            <span className="text-xs font-semibold uppercase tracking-wider text-accent-primary">
              Archive
            </span>
            <h2 className="font-serif text-4xl font-normal tracking-tight text-text-primary mt-1 select-text selection:bg-accent-primary/20">
              Your archive.
            </h2>
            <p className="text-xs text-text-muted leading-normal mt-1 max-w-xl">
              {filesList.length > 0 ? `${filesList.length} file${filesList.length === 1 ? "" : "s"} indexed.` : "Start building your knowledge archive."}
            </p>
          </div>
          
          {/* List/Grid layout toggle switch */}
          {filesList.length > 0 && (
            <button
              onClick={toggleViewMode}
              className="p-2 border border-border-default hover:bg-bg-surface-raised text-text-muted hover:text-text-primary rounded-lg transition-all ease-motion cursor-pointer flex items-center gap-1.5 text-xs font-semibold"
              title={`Switch to ${viewMode === "grid" ? "List" : "Grid"} view`}
            >
              {viewMode === "grid" ? (
                <>
                  <List className="h-4.5 w-4.5" />
                  <span>List View</span>
                </>
              ) : (
                <>
                  <LayoutGrid className="h-4.5 w-4.5" />
                  <span>Grid View</span>
                </>
              )}
            </button>
          )}
        </div>

        {/* 1. Significa-style Stats metrics Row Grid with Shared dividers */}
        {!isLoading && filesList.length > 0 && (
          <div className="grid grid-cols-2 md:grid-cols-4 border border-border-default bg-bg-surface divide-y md:divide-y-0 md:divide-x divide-border-default select-none shadow-sm rounded-xl overflow-hidden animate-fade-in font-sans">
            <div className="p-6 flex flex-col items-center justify-center text-center bg-bg-base/30">
              <span className="text-xs font-semibold text-text-muted uppercase tracking-wide">Total Files</span>
              <span className="text-3xl font-serif font-normal text-text-primary mt-2">
                {usage?.fileCount ?? filesList.length}
              </span>
            </div>
            <div className="p-6 flex flex-col items-center justify-center text-center bg-bg-base/30">
              <span className="text-xs font-semibold text-text-muted uppercase tracking-wide">Passages Indexed</span>
              <span className="text-3xl font-serif font-normal text-text-primary mt-2">
                {usage?.chunkCount ?? 0}
              </span>
            </div>
            <div className="p-6 flex flex-col items-center justify-center text-center bg-bg-base/30">
              <span className="text-xs font-semibold text-text-muted uppercase tracking-wide">Storage Used</span>
              <span className="text-3xl font-serif font-normal text-text-primary mt-2">
                {usage ? formatBytes(usage.storageUsed) : "0 Bytes"}
              </span>
            </div>
            <div className="p-6 flex flex-col items-center justify-center text-center bg-bg-base/30">
              <span className="text-xs font-semibold text-text-muted uppercase tracking-wide">Ready to Search</span>
              <span className="text-3xl font-serif font-normal text-text-primary mt-2">
                {filesList.filter((f) => f.status === "completed").length} / {filesList.length}
              </span>
            </div>
          </div>
        )}

        {/* Active Indexing Banner displaying background queue progress */}
        {processingFiles.length > 0 && (
          <div className="bg-accent-subtle border border-accent-subtle-border rounded-xl p-4.5 flex items-center justify-between select-none animate-fade-in shadow-sm">
            <div className="flex items-center gap-3">
              <span className="relative flex h-2 w-2">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-accent-primary opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2 w-2 bg-accent-primary"></span>
              </span>
              <span className="text-xs font-semibold text-text-primary">
                Indexing {processingFiles.length} file{processingFiles.length === 1 ? "" : "s"}...
              </span>
            </div>
            <div className="h-1.5 w-32 bg-bg-base border border-border-default rounded-full overflow-hidden p-[1px] hidden sm:block">
              <div className="h-full bg-accent-primary rounded-full animate-pulse w-2/3" />
            </div>
          </div>
        )}

        {/* Failed Files Contextual Warning Alert Banner */}
        {failedFiles.length > 0 && (
          <div className="bg-state-error/10 border border-state-error/25 text-state-error text-xs rounded-xl p-4 flex items-center justify-between animate-fade-in shadow-sm select-none">
            <div className="flex items-center gap-3">
              <AlertCircle className="h-4.5 w-4.5 shrink-0" />
              <span className="font-semibold text-xs">
                {failedFiles.length} file{failedFiles.length === 1 ? "" : "s"} failed to index.
              </span>
            </div>
            <button
              onClick={() => {
                setSelectedFile(failedFiles[0].id, failedFiles[0].originalName);
              }}
              className="text-xs font-bold hover:underline cursor-pointer transition-colors"
            >
              View details →
            </button>
          </div>
        )}

        {/* Quick Capture Input thought clipper */}
        <form 
          onSubmit={handleCaptureSubmit} 
          className="bg-bg-surface border border-border-default hover:border-border-strong focus-within:border-text-primary focus-within:hover:border-text-primary p-1.5 rounded-xl shadow-sm flex items-center gap-2 select-none animate-fade-in font-sans transition-all"
        >
          <div className="pl-3.5 pr-1 text-text-muted flex items-center shrink-0">
            {isCapturing ? (
              <Loader2 className="h-4 w-4 animate-spin text-accent-primary" />
            ) : (
              <Zap className="h-4 w-4 text-accent-primary" />
            )}
          </div>
          <input
            type="text"
            value={captureInput}
            onChange={(e) => setCaptureInput(e.target.value)}
            disabled={isCapturing}
            placeholder="Capture a thought, note, or paste a URL..."
            className="flex-1 bg-transparent py-2.5 text-xs text-text-primary placeholder:text-text-dim focus:outline-none placeholder:font-medium disabled:opacity-50 font-medium"
          />
          <button
            type="submit"
            disabled={!captureInput.trim() || isCapturing}
            className="p-2.5 bg-accent-primary hover:bg-accent-hover text-bg-base rounded-lg transition-all shrink-0 cursor-pointer disabled:opacity-40 shadow-sm flex items-center justify-center"
            title="Ingest turning capture"
          >
            <ArrowRight className="h-3.5 w-3.5" />
          </button>
        </form>

        {/* Dashed Ingestion drop zone */}
        <UploadZone
          onUpload={handleUploadMultiple}
          isUploading={isUploading}
          uploadProgress={uploadProgress}
          uploadError={uploadError}
          onClearError={() => setUploadError(null)}
          maxSizeMB={50}
        />

        {/* Global Fetch Errors banner */}
        {fetchError && (
          <div className="bg-state-error/10 border border-state-error/25 text-state-error text-xs rounded-xl p-4 flex items-start gap-3">
            <AlertCircle className="h-5 w-5 mt-0.5 shrink-0" />
            <div>
              <p className="font-semibold uppercase tracking-wide text-[10px]">Could not load files</p>
              <p className="leading-relaxed mt-0.5 select-text selection:bg-accent-primary/20">
                Failed to connect. Check your connection.
              </p>
            </div>
          </div>
        )}

        {/* Continue Where You Left Off row (Recently opened files) */}
        {recentFiles.length > 0 && (
          <div className="space-y-4 animate-fade-in border-b border-border-default pb-8">
            <h3 className="text-xs font-semibold text-text-dim uppercase tracking-wider select-none">
              Recently Opened
            </h3>
            <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-6">
              {recentFiles.map((file) => (
                <div
                  key={file.id}
                  onClick={() => setSelectedFile(file.id, file.originalName)}
                  className="bg-bg-surface border border-border-default hover:border-border-strong rounded-xl p-4.5 transition-all flex items-center justify-between gap-4 cursor-pointer hover:bg-bg-surface-raised group select-none ease-motion"
                >
                  <div className="flex items-center gap-3.5 min-w-0">
                    <div className="p-2.5 bg-bg-base border border-border-default rounded-lg shrink-0 group-hover:border-border-strong group-hover:bg-bg-surface-raised transition-all">
                      {getFileTypeIcon(file.mimeType)}
                    </div>
                    <div className="min-w-0">
                      <p className="text-xs font-semibold text-text-primary truncate font-sans" title={file.originalName}>
                        {file.originalName}
                      </p>
                      <p className="text-xs text-text-muted mt-0.5">
                        Opened {new Date(file.lastOpenedAt!).toLocaleDateString(undefined, { dateStyle: "medium" })}
                      </p>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Pinned Files Row */}
        {pinnedFiles.length > 0 && (
          <div className="space-y-4 animate-fade-in border-b border-border-default pb-8">
            <div className="flex items-center gap-2">
              <Pin className="h-4 w-4 text-accent-primary" />
              <h3 className="text-xs font-semibold text-text-primary uppercase tracking-wider select-none">
                Pinned Files
              </h3>
            </div>
            <FileGrid isLoading={isLoading}>
              {pinnedFiles.map((file) => (
                <FileCard
                  key={file.id}
                  file={file}
                  onOpen={() => setSelectedFile(file.id, file.originalName)}
                  onDownload={() => handleDownload(file)}
                  onDelete={() => triggerDeleteConfirm(file)}
                  onToggleStar={() => toggleStar({ fileId: file.id, isStarred: !file.isStarred })}
                  onTogglePin={() => togglePin({ fileId: file.id, isPinned: !file.isPinned })}
                  onRename={async () => { queryClient.invalidateQueries({ queryKey: ["files"] }); }}
                  onTagClick={(tag) => setSelectedTagFilters((prev) => prev.includes(tag) ? prev.filter((t) => t !== tag) : [...prev, tag])}
                  collections={collectionsList}
                  onMoveToCollection={(colId) => handleMoveFileToCollection(file.id, colId)}
                  isDownloading={downloadingId === file.id}
                  isDeleting={isDeleting && fileIdToDelete === file.id}
                  isStarring={starringId === file.id}
                  isFocused={focusedCardId === file.id}
                  isSelected={selectedFileIds.includes(file.id)}
                  onSelectToggle={(_e) => {
                    setSelectedFileIds((prev) =>
                      prev.includes(file.id) ? prev.filter((id) => id !== file.id) : [...prev, file.id]
                    );
                  }}
                />
              ))}
            </FileGrid>
          </div>
        )}

        {/* Visual Ingest List Segment */}
        <div className="space-y-4">
          <div className="flex justify-between items-center select-none pb-2">
            <h3 className="text-xs font-semibold text-text-muted">
              Files ({regularFiles.length})
            </h3>
          </div>

          {/* Quick Search & Tag Filters Bar */}
          {filesList.length > 0 && (
            <div className="space-y-4 border-b border-border-default pb-6 select-none">
              <div className="relative group/filter select-none">
                <input
                  type="text"
                  value={quickFilter}
                  onChange={(e) => setQuickFilter(e.target.value)}
                  placeholder="Quick filter files in grid..."
                  className="w-full bg-bg-surface border border-border-default hover:border-border-strong focus:border-text-primary focus:outline-none rounded-xl p-3.5 text-xs text-text-primary transition-all font-sans placeholder:text-text-dim"
                />
                {quickFilter && (
                  <button
                    onClick={() => setQuickFilter("")}
                    className="absolute right-4 top-3.5 p-1 text-xs text-state-error hover:underline transition-colors cursor-pointer"
                  >
                    Clear
                  </button>
                )}
              </div>

              {/* Tag Filter Pills */}
              {allTags.length > 0 && (
                <div className="flex flex-wrap items-center gap-1.5 select-none animate-fade-in pl-1">
                  <span className="text-xs font-semibold text-text-dim uppercase mr-1 select-none">Tags:</span>
                  {allTags.map((tag) => {
                    const isSelected = selectedTagFilters.includes(tag);
                    return (
                      <button
                        key={tag}
                        onClick={() => {
                          setSelectedTagFilters(prev => 
                            prev.includes(tag) 
                              ? prev.filter(t => t !== tag) 
                              : [...prev, tag]
                          );
                        }}
                        className={`px-3 py-1 rounded border text-xs font-semibold uppercase tracking-wider transition-all ease-motion cursor-pointer select-none ${
                          isSelected 
                            ? "bg-accent-primary text-bg-base border-accent-primary" 
                            : "bg-bg-base border-border-default text-text-muted hover:border-border-strong hover:text-text-primary"
                        }`}
                      >
                        {tag}
                      </button>
                    );
                  })}
                  {selectedTagFilters.length > 0 && (
                    <button
                      onClick={() => setSelectedTagFilters([])}
                      className="text-xs text-state-error hover:underline ml-2 cursor-pointer transition-colors"
                    >
                      Clear all
                    </button>
                  )}
                </div>
              )}

              {/* Sort Controls */}
              <div className="flex flex-wrap items-center gap-3 select-none pl-1 pt-2 border-t border-border-default/30">
                <span className="text-xs font-semibold text-text-dim uppercase mr-1 select-none">Sort By:</span>
                <div className="flex items-center gap-2">
                  <select
                    value={sortBy}
                    onChange={(e) => setSortBy(e.target.value as any)}
                    className="bg-bg-base border border-border-default hover:border-border-strong text-text-primary text-xs font-semibold px-2 py-1.5 rounded-lg focus:outline-none cursor-pointer"
                  >
                    <option value="date">Date Indexed</option>
                    <option value="name">Filename</option>
                    <option value="size">File Size</option>
                    <option value="type">File Type</option>
                    <option value="status">Index Status</option>
                  </select>
                  <button
                    onClick={() => setSortOrder(prev => prev === "asc" ? "desc" : "asc")}
                    className="px-2.5 py-1.5 border border-border-default hover:border-border-strong hover:bg-bg-surface-raised rounded-lg text-xs font-semibold text-text-muted hover:text-text-primary transition-all cursor-pointer"
                  >
                    {sortOrder === "asc" ? "Ascending ↑" : "Descending ↓"}
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* Persistent List / Grid Render Toggle block */}
          {viewMode === "list" && regularFiles.length > 0 ? (
            <div className="border border-border-default bg-bg-surface rounded-xl overflow-hidden divide-y divide-border-default/50 animate-fade-in font-sans shadow-sm">
              {paginatedRegularFiles.map((file) => {
                const isFocused = focusedCardId === file.id;
                const isSel = selectedFileIds.includes(file.id);
                return (
                  <div
                    key={file.id}
                    onClick={() => setSelectedFile(file.id, file.originalName)}
                    className={`flex flex-col sm:flex-row items-start sm:items-center justify-between p-4.5 transition-all gap-4 cursor-pointer hover:bg-bg-surface-raised group select-none ease-motion border-l-2 relative ${
                      isFocused ? "border-l-accent-primary bg-bg-base/25" : "border-l-transparent"
                    }`}
                  >
                    <div className="flex items-center gap-3.5 min-w-0 flex-1">
                      {/* Checkbox selector in list view */}
                      <div 
                        onClick={(e) => {
                          e.stopPropagation();
                          setSelectedFileIds((prev) =>
                            prev.includes(file.id) ? prev.filter((id) => id !== file.id) : [...prev, file.id]
                          );
                        }}
                        className={`h-4 w-4 border rounded flex items-center justify-center shrink-0 transition-all duration-[120ms] ease-motion cursor-pointer z-20 ${
                          isSel
                            ? "bg-accent-primary border-accent-primary text-bg-base animate-scale-in"
                            : "border-border-default hover:border-border-strong opacity-0 group-hover:opacity-100 bg-bg-surface"
                        }`}
                      >
                        {isSel && <Check className="h-3 w-3 stroke-[3] text-bg-base" />}
                      </div>
                      <div className="p-2 bg-bg-base border border-border-default rounded-lg shrink-0 group-hover:bg-bg-surface-raised transition-all">
                        {getFileTypeIcon(file.mimeType)}
                      </div>
                      <div className="min-w-0 flex-1 flex flex-col md:flex-row md:items-center gap-1 md:gap-4">
                        <span className="font-sans text-sm font-medium text-text-primary truncate max-w-sm" title={file.originalName}>
                          {file.originalName}
                        </span>
                        <div className="flex items-center gap-2 text-xs text-text-muted shrink-0">
                          <span>{formatBytes(file.sizeBytes)}</span>
                          <span>·</span>
                          <span>{new Date(file.createdAt).toLocaleDateString(undefined, { dateStyle: "short" })}</span>
                        </div>
                      </div>
                    </div>
                    
                    <div className="flex items-center gap-3 shrink-0">
                      {/* Tags */}
                      {file.tags && file.tags.length > 0 && (
                        <div className="hidden lg:flex items-center gap-1.5">
                          {file.tags.slice(0, 2).map(t => (
                            <span key={t} className="px-2 py-0.5 rounded bg-bg-base border border-border-default text-text-muted text-xs truncate max-w-[100px]">{t}</span>
                          ))}
                        </div>
                      )}
                      
                      {/* Status */}
                      {getStatusBadge(file.status)}
                      
                      {/* Actions */}
                      <div className="flex items-center gap-1.5 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none group-hover:pointer-events-auto">
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            toggleStar({ fileId: file.id, isStarred: !file.isStarred });
                          }}
                          disabled={starringId === file.id}
                          className="p-1 rounded-lg border border-border-default hover:text-amber-400 hover:border-border-strong text-text-muted bg-bg-base transition-all cursor-pointer"
                        >
                          <Star className={`h-3.5 w-3.5 ${file.isStarred ? "text-amber-400 fill-amber-400" : ""}`} />
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            handleDownload(file);
                          }}
                          disabled={downloadingId === file.id}
                          className="p-1 rounded-lg border border-border-default hover:text-accent-primary hover:border-border-strong text-text-muted bg-bg-base transition-all cursor-pointer"
                        >
                          <Download className="h-3.5 w-3.5" />
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            triggerDeleteConfirm(file);
                          }}
                          disabled={isDeleting}
                          className="p-1 rounded-lg border border-border-default hover:text-state-error hover:border-state-error/25 text-text-muted bg-bg-base transition-all cursor-pointer"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          ) : (
            <FileGrid isLoading={isLoading}>
              {paginatedRegularFiles.map((file) => (
                <FileCard
                  key={file.id}
                  file={file}
                  onOpen={() => setSelectedFile(file.id, file.originalName)}
                  onDownload={() => handleDownload(file)}
                  onDelete={() => triggerDeleteConfirm(file)}
                  onToggleStar={() => toggleStar({ fileId: file.id, isStarred: !file.isStarred })}
                  onTogglePin={() => togglePin({ fileId: file.id, isPinned: !file.isPinned })}
                  onRename={async () => { queryClient.invalidateQueries({ queryKey: ["files"] }); }}
                  onTagClick={(tag) => setSelectedTagFilters((prev) => prev.includes(tag) ? prev.filter((t) => t !== tag) : [...prev, tag])}
                  collections={collectionsList}
                  onMoveToCollection={(colId) => handleMoveFileToCollection(file.id, colId)}
                  isDownloading={downloadingId === file.id}
                  isDeleting={isDeleting && fileIdToDelete === file.id}
                  isStarring={starringId === file.id}
                  isFocused={focusedCardId === file.id}
                  isSelected={selectedFileIds.includes(file.id)}
                  onSelectToggle={(_e) => {
                    setSelectedFileIds((prev) =>
                      prev.includes(file.id) ? prev.filter((id) => id !== file.id) : [...prev, file.id]
                    );
                  }}
                />
              ))}
            </FileGrid>
          )}

          {/* Pagination Controls */}
          {totalRegularPages > 1 && (
            <div className="flex items-center justify-between border-t border-border-default pt-6 font-sans select-none animate-fade-in">
              <span className="text-xs text-text-muted">
                Showing {Math.min(totalRegularItems, (currentPage - 1) * itemsPerPage + 1)}-{Math.min(totalRegularItems, currentPage * itemsPerPage)} of {totalRegularItems} files
              </span>
              <div className="flex items-center gap-1.5">
                <button
                  onClick={() => setCurrentPage(prev => Math.max(1, prev - 1))}
                  disabled={currentPage === 1}
                  className="px-3 py-1.5 border border-border-default rounded-lg text-xs font-semibold text-text-primary hover:bg-bg-surface-raised disabled:opacity-40 transition-all cursor-pointer bg-bg-surface"
                >
                  Previous
                </button>
                {Array.from({ length: totalRegularPages }).map((_, i) => (
                  <button
                    key={i}
                    onClick={() => setCurrentPage(i + 1)}
                    className={`px-3 py-1.5 border rounded-lg text-xs font-semibold transition-all cursor-pointer ${
                      currentPage === i + 1
                        ? "bg-accent-primary text-bg-base border-accent-primary"
                        : "border-border-default text-text-muted hover:border-border-strong hover:text-text-primary bg-bg-surface"
                    }`}
                  >
                    {i + 1}
                  </button>
                ))}
                <button
                  onClick={() => setCurrentPage(prev => Math.min(totalRegularPages, prev + 1))}
                  disabled={currentPage === totalRegularPages}
                  className="px-3 py-1.5 border border-border-default rounded-lg text-xs font-semibold text-text-primary hover:bg-bg-surface-raised disabled:opacity-40 transition-all cursor-pointer bg-bg-surface"
                >
                  Next
                </button>
              </div>
            </div>
          )}

          {!isLoading && filesList.length === 0 && (
            <div className="border border-dashed border-border-default rounded-xl p-10 md:p-16 flex flex-col items-center justify-center text-center bg-bg-surface/30 space-y-6">
              <div className="max-w-md space-y-3">
                <span className="text-xs font-semibold tracking-wide text-accent-primary block select-none uppercase">
                  Ready
                </span>
                <h4 className="font-sans text-2xl text-text-primary font-semibold">Start building your knowledge archive.</h4>
                <p className="text-xs text-text-muted leading-relaxed font-sans font-medium">
                  Once you upload files, you can search across all of them in natural language:
                </p>
              </div>

              {/* Terminal Query Previews */}
              <div className="w-full max-w-lg bg-bg-base border border-border-default rounded-xl p-4.5 text-left font-mono text-[11px] text-text-muted space-y-2.5 shadow-sm">
                <div className="flex items-center gap-1.5 pb-2 border-b border-border-default/50 text-text-dim select-none">
                  <span className="h-2 w-2 rounded-full bg-border-strong"></span>
                  <span className="h-2 w-2 rounded-full bg-border-strong"></span>
                  <span className="h-2 w-2 rounded-full bg-border-strong"></span>
                  <span className="ml-2 text-[10px] uppercase font-bold tracking-widest font-mono">Example searches</span>
                </div>
                <div className="flex items-center gap-2 text-text-primary">
                  <span className="text-accent-primary shrink-0">&gt;</span>
                  <span>"Find my CNN notes about residual networks"</span>
                </div>
                <div className="flex items-center gap-2 text-text-primary">
                  <span className="text-accent-primary shrink-0">&gt;</span>
                  <span>"Screenshots with deadlines"</span>
                </div>
                <div className="flex items-center gap-2 text-text-primary">
                  <span className="text-accent-primary shrink-0">&gt;</span>
                  <span>"What was I working on last week?"</span>
                </div>
              </div>

              <div className="text-xs font-semibold text-text-dim pt-2 select-none">
                Drag files above or drop them anywhere to start.
              </div>
            </div>
          )}
        </div>

        {/* Slide-in right panel file details sheet */}
        {selectedFileId && (
          <FileDetailSheet
            fileId={selectedFileId}
            fileName={selectedFileName}
            isOpen={!!selectedFileId}
            onClose={() => setSelectedFile(null)}
            onDelete={() => {
              const activeFile = filesList.find((f) => f.id === selectedFileId);
              if (activeFile) triggerDeleteConfirm(activeFile);
            }}
            onDownload={() => {
              const activeFile = filesList.find((f) => f.id === selectedFileId);
              if (activeFile) handleDownload(activeFile);
            }}
            isDownloading={downloadingId === selectedFileId}
            isDeleting={isDeleting && fileIdToDelete === selectedFileId}
          />
        )}

        {/* Styled confirm dialog replacement */}
        <Dialog
          isOpen={deleteConfirmOpen}
          title="Delete this file?"
          description={`"${fileNameToDelete}" will be permanently deleted. This cannot be undone.`}
          cancelText="Cancel"
          confirmText="Delete"
          onCancel={() => {
            setDeleteConfirmOpen(false);
            setFileIdToDelete(null);
            setFileNameToDelete("");
          }}
          onConfirm={executeDeletion}
          isConfirming={isDeleting}
        />

        {/* Styled bulk confirm dialog replacement */}
        <Dialog
          isOpen={bulkDeleteConfirmOpen}
          title={`Delete ${selectedFileIds.length} files?`}
          description={`${selectedFileIds.length} files will be permanently deleted. This cannot be undone.`}
          cancelText="Cancel"
          confirmText="Delete All"
          onCancel={() => setBulkDeleteConfirmOpen(false)}
          onConfirm={() => {
            bulkDelete(selectedFileIds);
            setBulkDeleteConfirmOpen(false);
          }}
        />

        {/* Floating Bulk Actions Bar */}
        {selectedFileIds.length > 0 && (
          <div className="fixed bottom-6 left-1/2 transform -translate-x-1/2 bg-bg-surface border border-border-strong px-6 py-4 rounded-xl shadow-2xl flex items-center gap-6 z-30 select-none animate-fade-in font-sans">
            <span className="text-xs font-semibold text-text-primary shrink-0 select-none">
              {selectedFileIds.length} file{selectedFileIds.length === 1 ? "" : "s"} selected
            </span>
            <div className="h-4 w-px bg-border-default shrink-0" />
            <div className="flex items-center gap-2">
              <button
                onClick={() => {
                  const filesToDownload = filesList.filter((f) => selectedFileIds.includes(f.id));
                  filesToDownload.forEach((file) => handleDownload(file));
                }}
                className="px-3 py-1.5 border border-border-default hover:border-border-strong text-text-primary rounded-lg text-xs font-semibold uppercase tracking-wider bg-bg-base hover:bg-bg-surface-raised cursor-pointer transition-all flex items-center gap-1.5"
              >
                <Download className="h-3.5 w-3.5" />
                Download
              </button>
              <button
                onClick={() => {
                  setBulkDeleteConfirmOpen(true);
                }}
                className="px-3 py-1.5 border border-transparent hover:bg-state-error/10 hover:border-state-error/25 text-state-error rounded-lg text-xs font-semibold uppercase tracking-wider cursor-pointer transition-all flex items-center gap-1.5"
              >
                <Trash2 className="h-3.5 w-3.5" />
                Delete
              </button>
              <button
                onClick={() => setSelectedFileIds([])}
                className="px-3 py-1.5 border border-border-default text-text-muted hover:text-text-primary rounded-lg text-xs font-semibold uppercase tracking-wider bg-transparent cursor-pointer transition-all"
              >
                Clear
              </button>
            </div>
          </div>
        )}

        {/* Styled general info/error dialog replacement */}
        <Dialog
          isOpen={infoDialog.open}
          title={infoDialog.title}
          description={infoDialog.description}
          cancelText=""
          confirmText="Dismiss"
          onCancel={() => setInfoDialog({ open: false, title: "", description: "" })}
          onConfirm={() => setInfoDialog({ open: false, title: "", description: "" })}
          variant="info"
        />

        {/* Keyboard Shortcuts Dialog */}
        <Dialog
          isOpen={shortcutsOpen}
          title="Keyboard Shortcuts"
          description="Navigate faster with keyboard shortcuts."
          cancelText=""
          confirmText="Got it"
          onCancel={() => setShortcutsOpen(false)}
          onConfirm={() => setShortcutsOpen(false)}
          variant="info"
        >
          <div className="grid grid-cols-2 gap-4 border border-border-default/50 rounded-xl p-4 bg-bg-base/30 text-xs font-sans text-text-primary mt-2">
            <div className="flex items-center justify-between border-b border-border-default/40 pb-2">
              <span className="font-semibold">Move selection down</span>
              <kbd className="px-2 py-0.5 border border-border-default rounded bg-bg-surface font-mono text-[10px]">J</kbd>
            </div>
            <div className="flex items-center justify-between border-b border-border-default/40 pb-2">
              <span className="font-semibold">Move selection up</span>
              <kbd className="px-2 py-0.5 border border-border-default rounded bg-bg-surface font-mono text-[10px]">K</kbd>
            </div>
            <div className="flex items-center justify-between border-b border-border-default/40 pb-2">
              <span className="font-semibold">Open details sheet</span>
              <kbd className="px-2 py-0.5 border border-border-default rounded bg-bg-surface font-mono text-[10px]">Enter</kbd>
            </div>
            <div className="flex items-center justify-between border-b border-border-default/40 pb-2">
              <span className="font-semibold">Download file</span>
              <kbd className="px-2 py-0.5 border border-border-default rounded bg-bg-surface font-mono text-[10px]">D</kbd>
            </div>
            <div className="flex items-center justify-between border-b border-border-default/40 pb-2">
              <span className="font-semibold">Delete file</span>
              <kbd className="px-2 py-0.5 border border-border-default rounded bg-bg-surface font-mono text-[10px]">Backspace / Del</kbd>
            </div>
            <div className="flex items-center justify-between border-b border-border-default/40 pb-2">
              <span className="font-semibold">Focus Search page</span>
              <kbd className="px-2 py-0.5 border border-border-default rounded bg-bg-surface font-mono text-[10px]">/</kbd>
            </div>
            <div className="flex items-center justify-between">
              <span className="font-semibold">Close panel</span>
              <kbd className="px-2 py-0.5 border border-border-default rounded bg-bg-surface font-mono text-[10px]">Esc</kbd>
            </div>
            <div className="flex items-center justify-between">
              <span className="font-semibold">Help shortcuts</span>
              <kbd className="px-2 py-0.5 border border-border-default rounded bg-bg-surface font-mono text-[10px] font-bold">?</kbd>
            </div>
          </div>
        </Dialog>

        {/* Google Drive-style Bottom-Right Batch Upload Queue Panel */}
        {activeUploads.length > 0 && (
          <div className="fixed bottom-6 right-6 w-80 md:w-96 bg-bg-surface border border-border-strong rounded-2xl shadow-2xl overflow-hidden z-40 animate-slide-in select-none font-sans">
            {/* Header */}
            <div className="bg-bg-base/80 border-b border-border-default px-4 py-3 flex items-center justify-between text-xs font-semibold select-none">
              <span className="text-text-primary">
                {activeUploads.some((u) => u.status === "uploading")
                  ? `Uploading ${activeUploads.filter((u) => u.status === "uploading").length} files...`
                  : "All uploads complete"}
              </span>
              <button 
                onClick={clearActiveUploads}
                className="text-text-dim hover:text-text-primary transition-colors cursor-pointer text-[10px] uppercase font-bold"
              >
                Close
              </button>
            </div>
            
            {/* Queue rows */}
            <div className="max-h-60 overflow-y-auto divide-y divide-border-default/50 p-2.5">
              {activeUploads.map((upload) => (
                <div key={upload.id} className="py-2.5 px-2.5 flex items-center justify-between gap-3 text-xs">
                  <div className="min-w-0 flex-1 space-y-1.5">
                    <div className="flex justify-between items-center text-[11px] font-sans font-medium text-text-primary">
                      <span className="truncate pr-4" title={upload.name}>{upload.name}</span>
                      <span className="shrink-0 text-text-muted font-mono">{upload.progress}%</span>
                    </div>
                    {upload.status === "uploading" && (
                      <div className="h-1 bg-bg-base rounded-full overflow-hidden p-[1px]">
                        <div 
                          className="h-full bg-accent-primary rounded-full transition-all duration-300"
                          style={{ width: `${upload.progress}%` }}
                        />
                      </div>
                    )}
                    {upload.error && (
                      <p className="text-[10px] text-state-error truncate">{upload.error}</p>
                    )}
                  </div>
                  
                  <div className="shrink-0">
                    {upload.status === "uploading" && (
                      <Loader2 className="h-3.5 w-3.5 animate-spin text-accent-primary" />
                    )}
                    {upload.status === "completed" && (
                      <Check className="h-3.5 w-3.5 text-state-success stroke-[3]" />
                    )}
                    {upload.status === "failed" && (
                      <AlertCircle className="h-3.5 w-3.5 text-state-error" />
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </ErrorBoundary>
  );
}
