import { useState, useEffect } from "react";
import { useParams, useNavigate, useSearchParams, Link } from "react-router";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { useAuthStore } from "../../store/auth.store";
import { useUIStore } from "../../store/ui.store";
import { 
  Folder, 
  Search, 
  Check, 
  Loader2
} from "lucide-react";
import { FileCard, FileItem } from "../../components/files/FileCard";
import { FileGrid } from "../../components/files/FileGrid";
import { FileDetailSheet } from "../../components/files/FileDetailSheet";
import { Dialog } from "../../components/ui/Dialog";
import { ErrorBoundary } from "../../components/ui/ErrorBoundary";
import { GroundedAnswer } from "../../components/search/GroundedAnswer";
import { SearchResultCard } from "../../components/search/SearchResultCard";
import { toast } from "sonner";

// Debounce hook
function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value);
  useEffect(() => {
    const handler = setTimeout(() => {
      setDebouncedValue(value);
    }, delay);
    return () => clearTimeout(handler);
  }, [value, delay]);
  return debouncedValue;
}

export function CollectionPage() {
  const { id: collectionId } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { accessToken } = useAuthStore();
  const queryClient = useQueryClient();

  const {
    selectedFileId,
    selectedFileName,
    setSelectedFile,
  } = useUIStore();

  const [searchParams, setSearchParams] = useSearchParams();
  const openFileId = searchParams.get("openFileId");

  // Local state
  const [isEditingName, setIsEditingName] = useState(false);
  const [editName, setEditName] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const debouncedQuery = useDebounce(searchQuery, 350);
  const [searchMode, setSearchMode] = useState<"keyword" | "semantic" | "hybrid">("hybrid");
  const [hasRequestedAI, setHasRequestedAI] = useState(false);
  
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [starringId, setStarringId] = useState<string | null>(null);
  const [focusedCardId, setFocusedCardId] = useState<string | null>(null);
  const [focusedResultId] = useState<string | null>(null);

  // Deletion dialog states
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [fileIdToDelete, setFileIdToDelete] = useState<string | null>(null);
  const [fileNameToDelete, setFileNameToDelete] = useState("");
  const [deleteColOpen, setDeleteColOpen] = useState(false);

  // Fetch collections
  const { data: collectionsData } = useQuery<{ data: Array<{ id: string; name: string; color: string }> }>({
    queryKey: ["collections"],
    queryFn: () => api.get("/api/v1/collections"),
  });
  const collectionsList = collectionsData?.data || [];
  const currentCollection = collectionsList.find((c) => c.id === collectionId);

  // Fetch all files
  const { data: filesData, isLoading: isFilesLoading } = useQuery<{ data: FileItem[] }>({
    queryKey: ["files"],
    queryFn: () => api.get("/api/v1/files"),
    refetchInterval: (query) => {
      const list = query.state.data?.data;
      if (!list) return false;
      const hasActive = list.some((f) => f.status === "pending" || f.status === "processing");
      return hasActive ? 3000 : false;
    },
  });
  const filesList = filesData?.data || [];
  // Filter to only this collection
  const collectionFiles = filesList.filter((f) => f.collectionId === collectionId);

  // Sync openFileId parameter
  useEffect(() => {
    if (openFileId && filesList.length > 0) {
      const file = filesList.find((f) => f.id === openFileId);
      if (file) {
        setSelectedFile(file.id, file.originalName);
        const next = new URLSearchParams(searchParams);
        next.delete("openFileId");
        setSearchParams(next, { replace: true });
      }
    }
  }, [openFileId, filesList, searchParams, setSearchParams, setSelectedFile]);

  // Set initial editing name
  useEffect(() => {
    if (currentCollection) {
      setEditName(currentCollection.name);
    }
  }, [currentCollection]);

  // Invalidate RAG when query shifts
  useEffect(() => {
    setHasRequestedAI(false);
  }, [debouncedQuery]);

  // Scoped search query fetch
  const { data: searchData, isFetching: isSearching } = useQuery({
    queryKey: ["collection-search", collectionId, debouncedQuery, searchMode],
    queryFn: () => {
      const params = new URLSearchParams({
        q: debouncedQuery,
        collectionId: collectionId || "",
        mode: searchMode,
        limit: "10",
      });
      return api.get(`/api/v1/search?${params.toString()}`);
    },
    enabled: !!collectionId && debouncedQuery.trim().length > 0,
  });

  const searchResults = (searchData as any)?.data?.results || [];

  // Update collection name/color mutation
  const { mutate: updateCollection } = useMutation({
    mutationFn: async (payload: { name?: string; color?: string }) => {
      return api.patch(`/api/v1/collections/${collectionId}`, payload);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["collections"] });
      toast.success("Collection updated.");
      setIsEditingName(false);
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to update collection.");
    },
  });

  // Delete collection mutation
  const { mutate: deleteCol } = useMutation({
    mutationFn: async () => {
      return api.delete(`/api/v1/collections/${collectionId}`);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["collections"] });
      toast.success("Collection deleted successfully.");
      navigate("/dashboard");
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to delete collection.");
    },
  });

  // Move file to collection mutation
  const handleMoveFileToCollection = async (fileId: string, targetColId: string) => {
    try {
      await api.patch(`/api/v1/files/${fileId}`, { collectionId: targetColId });
      toast.success("File organized successfully.");
      queryClient.invalidateQueries({ queryKey: ["files"] });
    } catch (err: any) {
      toast.error(err.message || "Failed to organize file.");
    }
  };

  // Star mutation
  const { mutate: toggleStar } = useMutation({
    mutationFn: async ({ fileId, isStarred }: { fileId: string; isStarred: boolean }) => {
      setStarringId(fileId);
      await api.patch(`/api/v1/files/${fileId}`, { isStarred });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["files"] });
      queryClient.invalidateQueries({ queryKey: ["collection-search"] });
    },
    onSettled: () => {
      setStarringId(null);
    },
  });

  // Pin mutation
  const { mutate: togglePin } = useMutation({
    mutationFn: async ({ fileId, isPinned }: { fileId: string; isPinned: boolean }) => {
      await api.patch(`/api/v1/files/${fileId}`, { isPinned });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["files"] });
    },
  });

  // File delete mutation
  const { mutate: deleteFile, isPending: isDeletingFile } = useMutation({
    mutationFn: async (fileId: string) => {
      await api.delete(`/api/v1/files/${fileId}`);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["files"] });
      queryClient.invalidateQueries({ queryKey: ["collection-search"] });
      setDeleteConfirmOpen(false);
      setFileIdToDelete(null);
      setFileNameToDelete("");
      toast.success("File deleted successfully.");
      if (selectedFileId === fileIdToDelete) {
        setSelectedFile(null);
      }
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to delete file.");
      setDeleteConfirmOpen(false);
    },
  });

  // Secure download stream
  const handleDownload = async (fileId: string, fileName: string) => {
    if (downloadingId) return;
    setDownloadingId(fileId);
    try {
      const response = await fetch(`/api/v1/files/${fileId}/download`, {
        headers: {
          Authorization: `Bearer ${accessToken || ""}`,
        },
      });
      if (!response.ok) throw new Error("Download failed");
      const blob = await response.blob();
      const url = window.URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.setAttribute("download", fileName);
      document.body.appendChild(link);
      link.click();
      link.remove();
      window.URL.revokeObjectURL(url);
    } catch (err: any) {
      toast.error("Download failed. Physical file may be missing.");
    } finally {
      setDownloadingId(null);
    }
  };

  const handleRenameSubmit = () => {
    const trimmed = editName.trim();
    if (trimmed && currentCollection && trimmed !== currentCollection.name) {
      updateCollection({ name: trimmed });
    } else {
      setIsEditingName(false);
    }
  };

  const handleKeyDownName = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      handleRenameSubmit();
    } else if (e.key === "Escape") {
      setIsEditingName(false);
      if (currentCollection) setEditName(currentCollection.name);
    }
  };

  // Keyboard navigation
  useEffect(() => {
    const handleGlobalKey = (e: KeyboardEvent) => {
      if (
        document.activeElement?.tagName === "INPUT" ||
        document.activeElement?.tagName === "TEXTAREA" ||
        deleteConfirmOpen ||
        deleteColOpen
      ) {
        return;
      }
      if (collectionFiles.length === 0) return;
      const currentIndex = collectionFiles.findIndex((f) => f.id === focusedCardId);

      if (e.key === "j" || e.key === "ArrowDown") {
        e.preventDefault();
        const nextIndex = (currentIndex + 1) % collectionFiles.length;
        setFocusedCardId(collectionFiles[nextIndex].id);
      } else if (e.key === "k" || e.key === "ArrowUp") {
        e.preventDefault();
        const prevIndex = (currentIndex - 1 + collectionFiles.length) % collectionFiles.length;
        setFocusedCardId(collectionFiles[prevIndex].id);
      } else if (e.key === "Enter" && focusedCardId) {
        e.preventDefault();
        const file = collectionFiles.find((f) => f.id === focusedCardId);
        if (file) setSelectedFile(file.id, file.originalName);
      } else if (e.key === "Escape" && selectedFileId) {
        e.preventDefault();
        setSelectedFile(null);
      }
    };
    window.addEventListener("keydown", handleGlobalKey);
    return () => window.removeEventListener("keydown", handleGlobalKey);
  }, [focusedCardId, collectionFiles, deleteConfirmOpen, deleteColOpen, selectedFileId, setSelectedFile]);

  if (!currentCollection && !isFilesLoading) {
    return (
      <div className="p-8 text-center space-y-4">
        <Folder className="h-16 w-16 mx-auto text-text-muted opacity-50" />
        <h2 className="font-serif text-2xl text-text-primary">Collection not found</h2>
        <p className="text-sm text-text-muted">This folder does not exist or was deleted.</p>
        <Link to="/dashboard" className="text-xs uppercase font-bold text-accent-primary hover:underline">
          Go back to dashboard
        </Link>
      </div>
    );
  }

  const headingColor = currentCollection?.color || "#8b5cf6";

  return (
    <ErrorBoundary>
      <div className="space-y-8 select-none relative pb-16 font-sans">
        {/* Header section with Double click renaming & recolor options */}
        <div className="border-b border-border-default pb-6 flex flex-col md:flex-row md:items-end justify-between gap-4">
          <div className="flex-1 space-y-2">
            <div className="flex items-center gap-2">
              <span 
                className="h-3 w-3 rounded-full shrink-0 shadow-sm animate-scale" 
                style={{ backgroundColor: headingColor }} 
              />
              <span className="text-xs font-semibold uppercase tracking-wider text-accent-primary">
                Collection Detail
              </span>
            </div>

            {isEditingName ? (
              <div className="flex items-center gap-2 mt-1">
                <input
                  type="text"
                  value={editName}
                  onChange={(e) => setEditName(e.target.value)}
                  onKeyDown={handleKeyDownName}
                  onBlur={handleRenameSubmit}
                  className="bg-bg-base border border-accent-primary focus:outline-none rounded px-3 py-1.5 text-2xl font-serif font-normal text-text-primary max-w-md w-full"
                  autoFocus
                />
                <button
                  onClick={handleRenameSubmit}
                  className="p-2 border border-border-strong bg-bg-surface-raised rounded-lg text-state-success hover:bg-bg-surface-hover"
                  title="Confirm edit"
                >
                  <Check className="h-4 w-4" />
                </button>
              </div>
            ) : (
              <h1 
                onDoubleClick={() => setIsEditingName(true)}
                className="font-serif text-4xl font-normal tracking-tight text-text-primary mt-1 select-text selection:bg-accent-primary/20 cursor-pointer flex items-center gap-2.5 group"
                title="Double-click to rename this collection"
              >
                {currentCollection?.name || "Folder"}
                <span className="text-[10px] text-text-dim border border-border-default px-2 py-0.5 rounded uppercase font-bold tracking-widest font-sans opacity-0 group-hover:opacity-100 transition-opacity">
                  Double-click to Rename
                </span>
              </h1>
            )}

            <p className="text-xs text-text-muted">
              {collectionFiles.length} file{collectionFiles.length === 1 ? "" : "s"} inside this partitioning space.
            </p>
          </div>

          <div className="flex flex-wrap items-center gap-3 shrink-0">
            {/* Color picker box */}
            <div className="flex items-center gap-1.5 bg-bg-surface border border-border-default px-3 py-2 rounded-xl">
              <span className="text-[10px] uppercase font-bold tracking-wider text-text-muted mr-1">Color:</span>
              {["#ef4444", "#3b82f6", "#10b981", "#f59e0b", "#8b5cf6", "#ededed"].map((c) => (
                <button
                  key={c}
                  type="button"
                  onClick={() => updateCollection({ color: c })}
                  className={`h-4.5 w-4.5 rounded-full shrink-0 transition-transform hover:scale-110 cursor-pointer ${headingColor === c ? "ring-2 ring-offset-2 ring-text-primary scale-115" : ""}`}
                  style={{ backgroundColor: c }}
                  title="Recolor space"
                />
              ))}
            </div>

            <button
              onClick={() => setDeleteColOpen(true)}
              className="px-4 py-2 border border-state-error/20 hover:border-state-error/45 text-state-error hover:bg-state-error/5 text-xs font-semibold uppercase tracking-wider rounded-xl transition-all cursor-pointer"
              title="Delete collection folder"
            >
              Delete Folder
            </button>
          </div>
        </div>

        {/* Scoped semantic search container */}
        <div className="bg-bg-surface border border-border-default p-6 rounded-2xl space-y-4 shadow-sm select-none">
          <div className="flex items-center justify-between border-b border-border-default/50 pb-3">
            <div className="flex items-center gap-2">
              <Search className="h-4.5 w-4.5 text-text-muted" />
              <h3 className="text-xs font-semibold text-text-primary uppercase tracking-wider">
                Scoped Retrieval Engine
              </h3>
            </div>
            <div className="flex gap-1.5 select-none font-mono">
              {(["keyword", "semantic", "hybrid"] as const).map((m) => (
                <button
                  key={m}
                  onClick={() => setSearchMode(m)}
                  className={`px-2 py-0.5 rounded text-[10px] uppercase font-bold tracking-wider transition-all border cursor-pointer ${searchMode === m ? "bg-text-primary text-bg-base border-text-primary" : "text-text-muted border-border-default hover:text-text-primary"}`}
                >
                  {m}
                </button>
              ))}
            </div>
          </div>

          <div className="relative">
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={`Search semantically across files in ${currentCollection?.name}...`}
              className="w-full bg-bg-base border border-border-default hover:border-border-strong focus:border-text-primary focus:outline-none rounded-xl p-3.5 text-xs text-text-primary transition-all font-sans placeholder:text-text-dim"
            />
            {searchQuery && (
              <button
                onClick={() => setSearchQuery("")}
                className="absolute right-4 top-3.5 text-xs text-state-error hover:underline"
              >
                Clear
              </button>
            )}
          </div>

          {/* Scoped AI answer panel */}
          {debouncedQuery.trim().length > 0 && (
            <div className="pt-2">
              <GroundedAnswer
                query={debouncedQuery}
                searchMode={searchMode}
                collectionId={collectionId}
                hasRequestedAI={hasRequestedAI}
                onRequestAI={setHasRequestedAI}
                onCitationClick={(citation) => setSelectedFile(citation.fileId, citation.fileName)}
              />
            </div>
          )}

          {/* Results displaying */}
          {debouncedQuery.trim().length > 0 && (
            <div className="space-y-4 pt-4 border-t border-border-default/30">
              <div className="flex justify-between items-center text-[10px] font-bold uppercase tracking-widest text-text-dim select-none">
                <span>Matching Passages ({searchResults.length})</span>
                {isSearching && <Loader2 className="h-3 w-3 animate-spin text-text-primary" />}
              </div>

              <div className="space-y-3.5 max-h-96 overflow-y-auto pr-1">
                {searchResults.map((result: any) => (
                  <SearchResultCard
                    key={result.chunkId}
                    result={result}
                    onOpen={() => setSelectedFile(result.fileId, result.fileName)}
                    isFocused={focusedResultId === result.chunkId}
                  />
                ))}
                {searchResults.length === 0 && !isSearching && (
                  <div className="p-8 text-center text-xs text-text-dim font-medium bg-bg-base/30 rounded-xl border border-border-default/60">
                    No passages matched your query inside this workspace.
                  </div>
                )}
              </div>
            </div>
          )}
        </div>

        {/* Collection files listing grid */}
        <div className="space-y-4">
          <h3 className="text-xs font-semibold text-text-dim uppercase tracking-wider select-none px-1">
            Files in Collection ({collectionFiles.length})
          </h3>

          <FileGrid isLoading={isFilesLoading}>
            {collectionFiles.map((file) => (
              <FileCard
                key={file.id}
                file={file}
                onOpen={() => setSelectedFile(file.id, file.originalName)}
                onDownload={() => handleDownload(file.id, file.originalName)}
                onDelete={() => {
                  setFileIdToDelete(file.id);
                  setFileNameToDelete(file.originalName);
                  setDeleteConfirmOpen(true);
                }}
                onToggleStar={() => toggleStar({ fileId: file.id, isStarred: !file.isStarred })}
                onTogglePin={() => togglePin({ fileId: file.id, isPinned: !file.isPinned })}
                onRename={async () => { queryClient.invalidateQueries({ queryKey: ["files"] }); }}
                collections={collectionsList}
                onMoveToCollection={(targetId) => handleMoveFileToCollection(file.id, targetId)}
                isDownloading={downloadingId === file.id}
                isDeleting={isDeletingFile && fileIdToDelete === file.id}
                isStarring={starringId === file.id}
                isFocused={focusedCardId === file.id}
              />
            ))}
          </FileGrid>

          {collectionFiles.length === 0 && !isFilesLoading && (
            <div className="border-2 border-dashed border-border-default rounded-2xl p-16 flex flex-col items-center justify-center text-center bg-bg-surface/30 select-none">
              <Folder className="h-12 w-12 text-text-muted opacity-40 mb-4" />
              <h4 className="font-serif text-lg text-text-primary">Empty Workspace Partition</h4>
              <p className="text-xs text-text-muted max-w-sm mt-1 mb-5">
                Organize your files by right-clicking file cards on the Dashboard and organizing them into this partition space.
              </p>
              <Link to="/dashboard" className="px-4 py-2 bg-bg-surface border border-border-default hover:border-border-strong text-text-primary hover:text-accent-primary text-xs font-semibold uppercase tracking-wider rounded-xl transition-all cursor-pointer shadow-sm">
                Back to dashboard
              </Link>
            </div>
          )}
        </div>

        {/* Slide-in right panel file detail sheet */}
        {selectedFileId && (
          <FileDetailSheet
            fileId={selectedFileId}
            fileName={selectedFileName}
            isOpen={!!selectedFileId}
            onClose={() => setSelectedFile(null)}
            onDelete={() => {
              setFileIdToDelete(selectedFileId);
              setFileNameToDelete(selectedFileName);
              setDeleteConfirmOpen(true);
            }}
            onDownload={() => handleDownload(selectedFileId, selectedFileName)}
            isDownloading={downloadingId === selectedFileId}
            isDeleting={isDeletingFile && fileIdToDelete === selectedFileId}
          />
        )}

        {/* Styled confirm delete file Dialog */}
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
          onConfirm={() => {
            if (fileIdToDelete) deleteFile(fileIdToDelete);
          }}
          isConfirming={isDeletingFile}
        />

        {/* Styled delete folder dialog */}
        <Dialog
          isOpen={deleteColOpen}
          title="Delete this collection?"
          description={`"${currentCollection?.name || "Folder"}" partition structure will be dropped. Files inside it will be released into your flat Dashboard archive. No files will be deleted physical storage.`}
          cancelText="Cancel"
          confirmText="Delete Folder"
          onCancel={() => setDeleteColOpen(false)}
          onConfirm={() => deleteCol()}
          variant="danger"
        />
      </div>
    </ErrorBoundary>
  );
}
