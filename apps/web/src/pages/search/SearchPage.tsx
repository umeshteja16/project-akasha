import { useState, useEffect, useRef } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router";
import { api } from "../../lib/api";
import { useAuthStore } from "../../store/auth.store";
import { useUIStore } from "../../store/ui.store";
import {
  Search as SearchIcon,
  Inbox,
  X,
  Sparkles,
  Bookmark,
} from "lucide-react";
import { toast } from "sonner";
import { SearchResultCard, SearchResult } from "../../components/search/SearchResultCard";
import { GroundedAnswer, GroundedCitation } from "../../components/search/GroundedAnswer";
import { FileDetailSheet } from "../../components/files/FileDetailSheet";
import { getFileTypeIcon } from "../../components/files/FileCard";
import { Dialog } from "../../components/ui/Dialog";
import { ErrorBoundary } from "../../components/ui/ErrorBoundary";

interface SearchResponse {
  data: {
    results: SearchResult[];
    pagination: {
      total: number;
      page: number;
      limit: number;
    };
    queryInfo?: {
      original: string;
      processed: string;
      wasExpanded: boolean;
      expansions: string[];
      suggestSemanticFallback?: boolean;
      spellSuggestion?: string | null;
    };
    telemetry?: {
      embedding_ms: number;
      db_search_ms: number;
      rerank_ms: number;
      total_ms: number;
    };
  };
}

// Custom hook to debounce inputs in React
function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value);

  useEffect(() => {
    const handler = setTimeout(() => {
      setDebouncedValue(value);
    }, delay);

    return () => {
      clearTimeout(handler);
    };
  }, [value, delay]);

  return debouncedValue;
}

export function SearchPage() {
  const { accessToken } = useAuthStore();
  const queryClient = useQueryClient();

  // Retrieve cached files list for zero-results suggestions
  const filesDataCached = queryClient.getQueryData<{ data: any[] }>(["files"]);
  const cachedFilesList = filesDataCached?.data || [];
  const recentUploads = [...cachedFilesList].slice(0, 3);
  const commonTags = Array.from(new Set(cachedFilesList.flatMap(f => f.tags || []).filter(Boolean))).slice(0, 6);

  const {
    selectedFileId,
    selectedFileName,
    setSelectedFile,
  } = useUIStore();

  const [searchParams, setSearchParams] = useSearchParams();

  // Local text input query so typing is smooth and lag-free
  const [queryInput, setQueryInput] = useState(() => searchParams.get("q") || "");
  const debouncedQuery = useDebounce(queryInput, 350);

  // Read search filters directly from URL query parameters
  const searchMode = (searchParams.get("mode") as "keyword" | "semantic" | "hybrid") || "hybrid";
  const selectedType = (searchParams.get("type") as "all" | "pdf" | "image" | "text" | "media") || "all";
  const selectedTimeRange = (searchParams.get("timeRange") as "all" | "day" | "week" | "month") || "all";
  const currentPage = parseInt(searchParams.get("page") || "1", 10);

  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  
  // RAG query requested state
  const [hasRequestedAI, setHasRequestedAI] = useState(false);
  // Telemetry diagnostics expanded state
  const [showDiagnostics, setShowDiagnostics] = useState(false);

  // Saved Searches state
  interface SavedSearchItem {
    query: string;
    mode: "keyword" | "semantic" | "hybrid";
    type: "all" | "pdf" | "image" | "text" | "media";
    timeRange: "all" | "day" | "week" | "month";
  }
  const [savedSearches, setSavedSearches] = useState<SavedSearchItem[]>([]);

  // Helper to update search params smoothly
  const updateParams = (updates: Record<string, string | number | undefined | null>) => {
    const next = new URLSearchParams(searchParams);
    Object.entries(updates).forEach(([key, val]) => {
      if (val === undefined || val === null || val === "" || val === "all" || (key === "page" && val === 1) || (key === "mode" && val === "hybrid")) {
        next.delete(key);
      } else {
        next.set(key, val.toString());
      }
    });
    setSearchParams(next, { replace: true });
  };

  // Sync back when URL changes directly (e.g. forward/back buttons)
  useEffect(() => {
    const urlQ = searchParams.get("q") || "";
    if (urlQ !== queryInput) {
      setQueryInput(urlQ);
    }
  }, [searchParams]);

  // Sync debounced search to URL
  useEffect(() => {
    const urlQ = searchParams.get("q") || "";
    if (debouncedQuery !== urlQ) {
      updateParams({ q: debouncedQuery, page: 1 });
    }
  }, [debouncedQuery]);

  // Load saved searches from localStorage
  useEffect(() => {
    try {
      const stored = localStorage.getItem("akasha_saved_searches");
      if (stored) {
        setSavedSearches(JSON.parse(stored));
      }
    } catch {}
  }, []);

  const handleSaveSearch = () => {
    const qTrim = queryInput.trim();
    if (!qTrim) return;

    const exists = savedSearches.some(
      (s) => s.query.toLowerCase() === qTrim.toLowerCase()
    );
    if (exists) {
      toast.info("Search query already bookmarked.");
      return;
    }

    const newItem: SavedSearchItem = {
      query: qTrim,
      mode: searchMode,
      type: selectedType,
      timeRange: selectedTimeRange,
    };

    const updated = [newItem, ...savedSearches];
    setSavedSearches(updated);
    localStorage.setItem("akasha_saved_searches", JSON.stringify(updated));
    toast.success("Search saved.");
  };

  const handleDeleteSavedSearch = (e: React.MouseEvent, q: string) => {
    e.stopPropagation();
    const updated = savedSearches.filter((s) => s.query !== q);
    setSavedSearches(updated);
    localStorage.setItem("akasha_saved_searches", JSON.stringify(updated));
    toast.success("Saved search removed.");
  };

  // Rotating placeholders loop
  const placeholders = [
    "Find my CNN notes about residual networks",
    "Screenshots with deadlines",
    "What was I working on last week?"
  ];
  const [placeholderIndex, setPlaceholderIndex] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setPlaceholderIndex((prev) => (prev + 1) % placeholders.length);
    }, 4000);
    return () => clearInterval(interval);
  }, []);

  // Search input reference
  const searchInputRef = useRef<HTMLInputElement>(null);
  const navigate = useNavigate();

  // Search history state
  interface HistoryItem {
    query: string;
    mode: "keyword" | "semantic" | "hybrid";
    timestamp: number;
  }
  const [searchHistory, setSearchHistory] = useState<HistoryItem[]>([]);
  const [isSearchFocused, setIsSearchFocused] = useState(false);

  // Focus result items state
  const [focusedResultId, setFocusedResultId] = useState<string | null>(null);

  // States to control the general info/alert dialog
  const [infoDialog, setInfoDialog] = useState<{ open: boolean; title: string; description: string }>({
    open: false,
    title: "",
    description: "",
  });

  // States to control the styled delete confirm dialog (since file delete option is in side detail sheet)
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [fileIdToDelete, setFileIdToDelete] = useState<string | null>(null);
  const [fileNameToDelete, setFileNameToDelete] = useState("");

  // Determine starting date threshold based on selection
  let fromDateString: string | undefined = undefined;
  if (selectedTimeRange !== "all") {
    const date = new Date();
    if (selectedTimeRange === "day") date.setDate(date.getDate() - 1);
    else if (selectedTimeRange === "week") date.setDate(date.getDate() - 7);
    else if (selectedTimeRange === "month") date.setMonth(date.getMonth() - 1);
    fromDateString = date.toISOString();
  }

  // Reset opt-in status and thread when the search query changes
  useEffect(() => {
    setHasRequestedAI(false);
  }, [debouncedQuery]);

  // Load search history from localStorage
  useEffect(() => {
    try {
      const stored = localStorage.getItem("search_history");
      if (stored) {
        setSearchHistory(JSON.parse(stored));
      }
    } catch (err) {
      console.error("Failed to load search history", err);
    }
  }, []);

  // Append new search query to local storage
  useEffect(() => {
    if (debouncedQuery.trim()) {
      setSearchHistory((prev) => {
        const filtered = prev.filter((item) => item.query.toLowerCase() !== debouncedQuery.trim().toLowerCase());
        const updated = [
          { query: debouncedQuery.trim(), mode: searchMode, timestamp: Date.now() },
          ...filtered
        ].slice(0, 20);
        localStorage.setItem("search_history", JSON.stringify(updated));
        return updated;
      });
    }
  }, [debouncedQuery, searchMode]);

  // Handle focusing from URL query params
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.get("focus") === "true") {
      searchInputRef.current?.focus();
      navigate("/search", { replace: true });
    }
  }, [navigate]);

  // Listen for / globally to focus input
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
        searchInputRef.current?.focus();
      }
    };
    window.addEventListener("keydown", handleGlobalKey);
    return () => window.removeEventListener("keydown", handleGlobalKey);
  }, []);


  const handleRemoveHistoryItem = (e: React.MouseEvent, q: string) => {
    e.stopPropagation();
    setSearchHistory((prev) => {
      const updated = prev.filter((item) => item.query !== q);
      localStorage.setItem("search_history", JSON.stringify(updated));
      return updated;
    });
  };

  const handleClearHistory = (e: React.MouseEvent) => {
    e.stopPropagation();
    setSearchHistory([]);
    localStorage.removeItem("search_history");
  };

  // Fetch search results with TanStack Query
  const { data: searchData, isFetching } = useQuery<SearchResponse>({
    queryKey: ["search", debouncedQuery, selectedType, fromDateString, currentPage, searchMode],
    queryFn: () => {
      const searchParams = new URLSearchParams({
        q: debouncedQuery,
        page: currentPage.toString(),
        limit: "10",
        type: selectedType,
        mode: searchMode,
      });
      if (fromDateString) {
        searchParams.append("from", fromDateString);
      }
      return api.get(`/api/v1/search?${searchParams.toString()}`);
    },
    enabled: debouncedQuery.trim().length >= 1,
    placeholderData: (prev) => prev, // Keep previous results while loading new search query
  });

  const results = searchData?.data?.results || [];
  const pagination = searchData?.data?.pagination || { total: 0, page: 1, limit: 10 };
  const totalPages = Math.ceil(pagination.total / pagination.limit);

  // Delete Mutation
  const { mutate: deleteFile, isPending: isDeleting } = useMutation({
    mutationFn: async (fileId: string) => {
      await api.delete(`/api/v1/files/${fileId}`);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["search"] });
      setDeleteConfirmOpen(false);
      setFileIdToDelete(null);
      setFileNameToDelete("");

      if (selectedFileId && selectedFileId === fileIdToDelete) {
        setSelectedFile(null);
      }
    },
    onError: (err: any) => {
      setInfoDialog({
        open: true,
        title: "Delete failed",
        description: err.message || "Failed to delete file.",
      });
      setDeleteConfirmOpen(false);
    },
  });

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
      setInfoDialog({
        open: true,
        title: "Download failed",
        description: "The file could not be downloaded. It may have been moved or deleted.",
      });
    } finally {
      setDownloadingId(null);
    }
  };

  const handleCitationClick = (citation: GroundedCitation) => {
    const cardId = `chunk-${citation.chunkId}`;
    const element = document.getElementById(cardId);
    
    if (element) {
      element.scrollIntoView({ behavior: "smooth", block: "center" });
      element.classList.add("ring-2", "ring-accent-primary", "scale-[1.01]", "shadow-2xl");
      setTimeout(() => {
        element.classList.remove("ring-2", "ring-accent-primary", "scale-[1.01]", "shadow-2xl");
      }, 3000);
    } else {
      setSelectedFile(citation.fileId, citation.fileName);
    }
  };

  const triggerDeleteConfirm = (id: string, name: string) => {
    setFileIdToDelete(id);
    setFileNameToDelete(name);
    setDeleteConfirmOpen(true);
  };

  const executeDeletion = () => {
    if (fileIdToDelete) {
      deleteFile(fileIdToDelete);
    }
  };

  // j/k selector keys handler inside search results list
  useEffect(() => {
    const handleGlobalKey = (e: KeyboardEvent) => {
      if (
        document.activeElement?.tagName === "INPUT" ||
        document.activeElement?.tagName === "TEXTAREA" ||
        deleteConfirmOpen
      ) {
        return;
      }

      if (results.length === 0) return;
      const currentIndex = results.findIndex((r) => r.chunkId === focusedResultId);

      if (e.key === "j" || e.key === "ArrowDown") {
        e.preventDefault();
        const nextIndex = (currentIndex + 1) % results.length;
        setFocusedResultId(results[nextIndex].chunkId);
      } else if (e.key === "k" || e.key === "ArrowUp") {
        e.preventDefault();
        const prevIndex = (currentIndex - 1 + results.length) % results.length;
        setFocusedResultId(results[prevIndex].chunkId);
      } else if (e.key === "Enter") {
        if (focusedResultId) {
          e.preventDefault();
          const match = results.find((r) => r.chunkId === focusedResultId);
          if (match) setSelectedFile(match.fileId, match.fileName);
        }
      } else if (e.key === "d") {
        if (focusedResultId) {
          e.preventDefault();
          const match = results.find((r) => r.chunkId === focusedResultId);
          if (match) handleDownload(match.fileId, match.fileName);
        }
      } else if (e.key === "Delete" || e.key === "Backspace") {
        if (focusedResultId) {
          e.preventDefault();
          const match = results.find((r) => r.chunkId === focusedResultId);
          if (match) triggerDeleteConfirm(match.fileId, match.fileName);
        }
      } else if (e.key === "Escape" && selectedFileId) {
        e.preventDefault();
        setSelectedFile(null);
      }
    };

    window.addEventListener("keydown", handleGlobalKey);
    return () => window.removeEventListener("keydown", handleGlobalKey);
  }, [focusedResultId, results, deleteConfirmOpen, selectedFileId, setSelectedFile]);

  return (
    <ErrorBoundary>
      <div className="space-y-12 select-none">
        {/* Page Header - Stark Minimal Style */}
        <div className="border-b border-border-default pb-6 select-none">
          <span className="text-xs font-semibold uppercase tracking-wider text-accent-primary">
            Search
          </span>
          <h1 className="font-serif text-4xl font-normal tracking-tight text-text-primary mt-1 select-text selection:bg-accent-primary/20">
            Search your archive.
          </h1>
        </div>

        {/* Primary Search Input - Clean Stark Sharp Border Box */}
        <div role="search" className="relative group/search select-none z-30">
          <div className="relative flex items-center bg-bg-surface border border-border-default hover:border-border-strong focus-within:border-text-primary focus-within:hover:border-text-primary transition-all overflow-hidden p-1 select-none">
            <div className="pl-4 pr-2 text-text-muted flex items-center shrink-0">
              {isFetching ? (
                <Loader2 className="h-4.5 w-4.5 text-text-primary animate-spin" />
              ) : (
                <SearchIcon className="h-4.5 w-4.5" />
              )}
            </div>
            <input
              ref={searchInputRef}
              type="text"
              value={queryInput}
              onChange={(e) => setQueryInput(e.target.value)}
              onFocus={() => setIsSearchFocused(true)}
              onBlur={() => setTimeout(() => setIsSearchFocused(false), 200)}
              placeholder={placeholders[placeholderIndex]}
              className="w-full py-4 pr-20 text-sm text-text-primary bg-transparent focus:outline-none placeholder:text-text-dim font-sans selection:bg-accent-primary/20 font-medium"
            />
            
            <div className="absolute right-4 flex items-center gap-1">
              {queryInput.trim() && (
                <button
                  type="button"
                  onClick={handleSaveSearch}
                  className="p-1.5 text-text-muted hover:text-accent-primary hover:bg-bg-base transition-all cursor-pointer rounded-lg border border-transparent"
                  title="Bookmark this search"
                >
                  <Bookmark className="h-4 w-4" />
                </button>
              )}
              {queryInput && (
                <button
                  type="button"
                  onClick={() => setQueryInput("")}
                  className="p-1.5 text-text-muted hover:text-text-primary hover:bg-bg-base transition-all cursor-pointer rounded-lg border border-transparent"
                  title="Clear search"
                >
                  <X className="h-4 w-4" />
                </button>
              )}
            </div>
          </div>

          {/* Search History & Saved Searches focused empty state dropdown */}
          {isSearchFocused && !queryInput.trim() && (searchHistory.length > 0 || savedSearches.length > 0) && (
            <div 
              className="absolute left-0 right-0 top-full mt-1 bg-bg-surface border border-border-strong shadow-2xl z-40 overflow-hidden rounded-xl animate-fade-in font-sans p-2 space-y-3"
              onMouseDown={(e) => e.preventDefault()}
            >
              {/* Saved Searches Section */}
              {savedSearches.length > 0 && (
                <div className="space-y-1">
                  <div className="flex items-center gap-1.5 px-3 py-1 border-b border-border-default/50 text-[10px] font-mono text-accent-primary uppercase tracking-wider select-none font-bold">
                    <Bookmark className="h-3 w-3 fill-accent-primary text-accent-primary" />
                    <span>Saved Searches</span>
                  </div>
                  <div className="max-h-40 overflow-y-auto">
                    {savedSearches.map((item) => (
                      <div
                        key={item.query}
                        onClick={() => {
                          setQueryInput(item.query);
                          updateParams({
                            q: item.query,
                            mode: item.mode,
                            type: item.type,
                            timeRange: item.timeRange,
                            page: 1,
                          });
                        }}
                        className="flex items-center justify-between px-3 py-2 hover:bg-bg-surface-raised cursor-pointer transition-all rounded-lg group/saved"
                      >
                        <div className="flex items-center gap-2 min-w-0">
                          <span className="text-[9px] font-mono uppercase bg-accent-subtle border border-accent-subtle-border text-accent-primary px-1.5 py-0.5 rounded tracking-wider select-none">
                            {item.mode}
                          </span>
                          <span className="text-[9px] font-mono uppercase bg-bg-base border border-border-default text-text-muted px-1.5 py-0.5 rounded tracking-wider select-none">
                            {item.type}
                          </span>
                          <span className="text-xs text-text-primary truncate font-semibold">{item.query}</span>
                        </div>
                        <button
                          type="button"
                          onClick={(e) => handleDeleteSavedSearch(e, item.query)}
                          className="p-1 text-text-muted hover:text-state-error hover:bg-bg-base/50 rounded transition-all opacity-0 group-hover/saved:opacity-100 cursor-pointer"
                          title="Remove bookmark"
                        >
                          <X className="h-3 w-3" />
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Recent Queries Section */}
              {searchHistory.length > 0 && (
                <div className="space-y-1">
                  <div className="flex justify-between items-center px-3 py-1 border-b border-border-default/50 text-[10px] font-mono text-text-dim uppercase tracking-wider select-none font-bold">
                    <span>Recent Queries</span>
                    <button 
                      type="button"
                      onClick={handleClearHistory}
                      className="hover:text-state-error transition-colors font-bold uppercase cursor-pointer"
                    >
                      Clear History
                    </button>
                  </div>
                  <div className="max-h-40 overflow-y-auto">
                    {searchHistory.map((item) => (
                      <div
                        key={item.query}
                        onClick={() => {
                          setQueryInput(item.query);
                          updateParams({
                            q: item.query,
                            mode: item.mode,
                            page: 1,
                          });
                        }}
                        className="flex items-center justify-between px-3 py-2 hover:bg-bg-surface-raised cursor-pointer transition-all rounded-lg group/hist"
                      >
                        <div className="flex items-center gap-2 min-w-0">
                          <span className="text-[9px] font-mono uppercase bg-bg-base border border-border-default text-text-muted px-1.5 py-0.5 rounded tracking-wider select-none">
                            {item.mode}
                          </span>
                          <span className="text-xs text-text-primary truncate font-medium">{item.query}</span>
                        </div>
                        <button
                          type="button"
                          onClick={(e) => handleRemoveHistoryItem(e, item.query)}
                          className="p-1 text-text-muted hover:text-state-error hover:bg-bg-base/50 rounded transition-all opacity-0 group-hover/hist:opacity-100 cursor-pointer"
                          title="Remove from history"
                        >
                          <X className="h-3 w-3" />
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Query expansion indicators */}
        {searchData?.data?.queryInfo?.wasExpanded && (
          <div className="border border-accent-subtle-border bg-accent-subtle px-4 py-3 rounded-none text-xs text-accent-primary flex items-center gap-2 animate-fade-in select-none">
            <span className="h-1.5 w-1.5 rounded-full bg-accent-primary animate-pulse" />
            <span>
              Showing results for <span className="font-semibold font-sans">"{searchData.data.queryInfo.processed}"</span> (expanded from <span className="italic font-sans">"{searchData.data.queryInfo.original}"</span>)
            </span>
          </div>
        )}

        {/* Spelling suggestion block */}
        {searchData?.data?.queryInfo?.spellSuggestion && (
          <div className="border border-amber-500/25 bg-amber-500/5 px-4.5 py-3.5 rounded-xl text-xs text-amber-500 flex items-center gap-2 animate-fade-in select-none">
            <span className="h-1.5 w-1.5 rounded-full bg-amber-500 animate-pulse" />
            <span>
              Did you mean:{" "}
              <button
                onClick={() => setQueryInput(searchData?.data?.queryInfo?.spellSuggestion || "")}
                className="font-bold underline hover:text-amber-600 transition-colors cursor-pointer"
              >
                {searchData?.data?.queryInfo?.spellSuggestion}
              </button>
              ?
            </span>
          </div>
        )}

        {/* Sleek Filtering Deck - Structural Grid Column Cards */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 bg-bg-surface/30 border border-border-default p-6 select-none font-sans">
          {/* Document Type Selector */}
          <div className="space-y-3.5 flex flex-col justify-between">
            <span className="text-[11px] uppercase font-bold tracking-widest text-text-muted font-mono block">File type</span>
            <div className="flex flex-wrap gap-1.5">
              {(["all", "pdf", "image", "text", "media"] as const).map((type) => (
                <button
                  key={type}
                  onClick={() => updateParams({ type, page: 1 })}
                  className={`px-3 py-1.5 border text-xs font-bold uppercase tracking-wider transition-all cursor-pointer rounded-none ${
                    selectedType === type
                      ? "bg-text-primary text-bg-base border-text-primary font-bold shadow-sm"
                      : "bg-transparent text-text-muted border-border-default hover:border-border-strong hover:text-text-primary"
                  }`}
                >
                  {type === "all" ? "All" : type === "pdf" ? "PDF" : type === "image" ? "Image" : type === "text" ? "Text" : "Media"}
                </button>
              ))}
            </div>
          </div>

          {/* Retrieval Mode Selector */}
          <div className="space-y-3.5 flex flex-col justify-between">
            <span className="text-[11px] uppercase font-bold tracking-widest text-text-muted font-mono block">Search mode</span>
            <div className="flex flex-wrap gap-1.5">
              {(["keyword", "semantic", "hybrid"] as const).map((modeOption) => (
                <button
                  key={modeOption}
                  onClick={() => updateParams({ mode: modeOption, page: 1 })}
                  className={`px-3 py-1.5 border text-xs font-bold uppercase tracking-wider transition-all cursor-pointer rounded-none ${
                    searchMode === modeOption
                      ? "bg-text-primary text-bg-base border-text-primary font-bold shadow-sm"
                      : "bg-transparent text-text-muted border-border-default hover:border-border-strong hover:text-text-primary"
                  }`}
                >
                  {modeOption}
                </button>
              ))}
            </div>
          </div>

          {/* Time Range Selector */}
          <div className="space-y-3.5 flex flex-col justify-between">
            <span className="text-[11px] uppercase font-bold tracking-widest text-text-muted font-mono block">Date range</span>
            <div className="flex flex-wrap gap-1.5">
              {(["all", "day", "week", "month"] as const).map((range) => (
                <button
                  key={range}
                  onClick={() => updateParams({ timeRange: range, page: 1 })}
                  className={`px-3 py-1.5 border text-xs font-bold uppercase tracking-wider transition-all cursor-pointer rounded-none ${
                    selectedTimeRange === range
                      ? "bg-text-primary text-bg-base border-text-primary font-bold shadow-sm"
                      : "bg-transparent text-text-muted border-border-default hover:border-border-strong hover:text-text-primary"
                  }`}
                >
                  {range === "all" ? "All Time" : range === "day" ? "Past 24h" : range === "week" ? "Week" : "Month"}
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* RAG Grounded Answer Panel (with explicit opt-in ASK AI trigger) */}
        {debouncedQuery.trim().length >= 1 && (
          <GroundedAnswer
            query={debouncedQuery}
            searchMode={searchMode}
            onCitationClick={handleCitationClick}
            hasRequestedAI={hasRequestedAI}
            onRequestAI={setHasRequestedAI}
          />
        )}

        {/* Results listing area */}
        {!queryInput.trim() ? (
          <div className="border border-border-default rounded-xl p-16 flex flex-col items-center justify-center text-center bg-bg-surface/30 shadow-sm select-none">
            <div className="h-16 w-16 rounded-xl bg-bg-surface border border-border-default flex items-center justify-center mb-5 text-text-muted">
              <SearchIcon className="h-7 w-7" />
            </div>
            <h4 className="font-serif text-xl text-text-primary">Search your files.</h4>
            <p className="text-xs text-text-muted max-w-sm mx-auto mt-2.5 leading-relaxed font-sans font-medium">
              Type above to search your indexed files.
            </p>
          </div>
        ) : isFetching && results.length === 0 ? (
          <div className="space-y-5">
            {[1, 2, 3].map((i) => (
              <div
                key={i}
                className="bg-bg-surface border border-border-default rounded-xl p-6 flex flex-col gap-4 animate-pulse select-none"
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3 w-1/2">
                    <div className="h-9 w-9 bg-bg-base border border-border-default rounded-xl shrink-0"></div>
                    <div className="flex-1 space-y-2">
                      <div className="h-3.5 bg-bg-base rounded-lg w-3/4"></div>
                      <div className="h-2.5 bg-bg-base rounded-lg w-1/3"></div>
                    </div>
                  </div>
                  <div className="h-4 bg-bg-base rounded-lg w-20"></div>
                </div>
                <div className="h-16 bg-bg-base rounded-lg w-full"></div>
              </div>
            ))}
          </div>
        ) : results.length === 0 ? (
          <div className="space-y-8 animate-fade-in font-sans select-none">
            <div className="border border-border-default rounded-xl p-12 flex flex-col items-center justify-center text-center bg-bg-surface/30 shadow-sm select-none space-y-4">
              <div className="h-12 w-12 rounded-xl bg-bg-surface border border-border-default flex items-center justify-center text-text-muted shrink-0 shadow-sm">
                <Inbox className="h-5 w-5" />
              </div>
              <div className="space-y-1.5">
                <h4 className="font-serif text-xl text-text-primary">No results found for "{queryInput}"</h4>
                <p className="text-xs text-text-muted max-w-md leading-relaxed font-sans font-medium">
                  No documents matched your search. Try different keywords, a broader date range, or switch to Semantic mode.
                </p>
              </div>

              {searchMode === "keyword" && searchData?.data?.queryInfo?.suggestSemanticFallback && (
                <button
                  onClick={() => updateParams({ mode: "semantic", page: 1 })}
                  className="px-4 py-2 border border-accent-subtle-border bg-accent-subtle hover:bg-accent-primary/20 text-accent-primary text-xs font-bold uppercase rounded-lg cursor-pointer transition-all shadow-sm select-none"
                >
                  Try Semantic Mode
                </button>
              )}
            </div>

            {/* Contextual suggestions deck */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6 select-none font-sans">
              {recentUploads.length > 0 && (
                <div className="border border-border-default rounded-xl p-5 bg-bg-surface/30 space-y-3.5 select-none shadow-sm">
                  <h5 className="text-[10px] uppercase font-bold tracking-widest text-text-muted font-mono block">
                    Recent uploads
                  </h5>
                  <div className="flex flex-col gap-2">
                    {recentUploads.map((file) => (
                      <div
                        key={file.id}
                        onClick={() => setSelectedFile(file.id, file.originalName)}
                        className="flex items-center justify-between p-2 rounded-lg bg-bg-base/30 hover:bg-bg-surface-raised transition-colors border border-border-default/50 cursor-pointer group"
                      >
                        <div className="flex items-center gap-2.5 min-w-0">
                          {getFileTypeIcon(file.mimeType)}
                          <span className="text-xs font-sans font-medium text-text-primary truncate" title={file.originalName}>
                            {file.originalName}
                          </span>
                        </div>
                        <span className="text-[9px] font-mono text-text-dim group-hover:text-text-primary transition-colors select-none uppercase tracking-wider">
                          View →
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {commonTags.length > 0 && (
                <div className="border border-border-default rounded-xl p-5 bg-bg-surface/30 space-y-3.5 select-none shadow-sm">
                  <h5 className="text-[10px] uppercase font-bold tracking-widest text-text-muted font-mono block">
                    Tags in your archive
                  </h5>
                  <div className="flex flex-wrap gap-1.5 pt-1 select-none">
                    {commonTags.map((tag) => (
                      <button
                        key={tag}
                        onClick={() => {
                          setQueryInput(tag);
                        }}
                        className="px-2.5 py-1 border border-border-default hover:border-border-strong text-text-muted hover:text-text-primary rounded text-[10px] font-semibold uppercase tracking-wider bg-bg-base/30 cursor-pointer transition-all duration-[120ms] ease-motion select-none"
                      >
                        {tag}
                      </button>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="space-y-6">
            <div className="flex justify-between items-center text-[11px] font-bold uppercase tracking-widest font-mono text-text-muted px-1 select-none">
              <div className="flex items-center gap-3">
                <span>Found {pagination.total} matching document{pagination.total === 1 ? "" : "s"}</span>
                {!hasRequestedAI && (
                  <button
                    onClick={() => setHasRequestedAI(true)}
                    className="inline-flex items-center gap-1.5 px-2.5 py-0.5 border border-accent-subtle-border bg-accent-subtle hover:bg-accent-primary/20 text-accent-primary text-[10px] font-bold uppercase rounded cursor-pointer transition-all"
                  >
                    <Sparkles className="h-3 w-3" />
                    Ask AI
                  </button>
                )}
              </div>
              <span>Page {pagination.page} of {totalPages || 1}</span>
            </div>

            {searchData?.data?.telemetry && (
              <div className="border-b border-border-default pb-4">
                <button
                  onClick={() => setShowDiagnostics(!showDiagnostics)}
                  className="flex items-center gap-1 text-[11px] font-mono text-text-muted hover:text-text-primary uppercase tracking-wider font-bold cursor-pointer"
                >
                  {showDiagnostics ? "Hide diagnostics ▴" : "Show diagnostics ▾"}
                </button>
                {showDiagnostics && (
                  <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 text-[11px] font-mono text-text-dim px-1 mt-2 select-none animate-fade-in uppercase">
                    <span>embedding: {searchData.data.telemetry.embedding_ms}ms</span>
                    <span>·</span>
                    <span>search: {searchData.data.telemetry.db_search_ms}ms</span>
                    <span>·</span>
                    <span>reranking: {searchData.data.telemetry.rerank_ms}ms</span>
                    <span>·</span>
                    <span className="font-bold text-text-primary">total: {searchData.data.telemetry.total_ms}ms</span>
                  </div>
                )}
              </div>
            )}

            <div aria-live="polite" className="space-y-4">
              {results.map((result) => (
                <SearchResultCard
                  key={result.chunkId}
                  result={result}
                  onOpen={() => setSelectedFile(result.fileId, result.fileName)}
                  isFocused={focusedResultId === result.chunkId}
                />
              ))}
            </div>

            {/* Pagination Controls */}
            {totalPages > 1 && (
              <div className="flex justify-center items-center gap-4 pt-4 select-none">
                <button
                  disabled={currentPage <= 1}
                  onClick={() => updateParams({ page: Math.max(currentPage - 1, 1) })}
                  className="px-4 py-2 border border-border-default hover:border-border-strong rounded-lg text-xs font-semibold uppercase tracking-wider text-text-muted hover:text-text-primary transition-all disabled:opacity-50 cursor-pointer"
                >
                  Previous
                </button>
                <span className="text-[10px] font-bold uppercase tracking-wider text-text-muted">
                  {currentPage} / {totalPages}
                </span>
                <button
                  disabled={currentPage >= totalPages}
                  onClick={() => updateParams({ page: Math.min(currentPage + 1, totalPages) })}
                  className="px-4 py-2 border border-border-default hover:border-border-strong rounded-lg text-xs font-semibold uppercase tracking-wider text-text-muted hover:text-text-primary transition-all disabled:opacity-50 cursor-pointer"
                >
                  Next
                </button>
              </div>
            )}
          </div>
        )}

        {/* Slide-in right panel file detail sheet */}
        {selectedFileId && (
          <FileDetailSheet
            fileId={selectedFileId}
            fileName={selectedFileName}
            isOpen={!!selectedFileId}
            onClose={() => setSelectedFile(null)}
            onDelete={() => triggerDeleteConfirm(selectedFileId, selectedFileName)}
            onDownload={() => handleDownload(selectedFileId, selectedFileName)}
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
      </div>
    </ErrorBoundary>
  );
}

// Loader icon for inline animations
function Loader2({ className }: { className?: string }) {
  return (
    <svg
      className={`animate-spin ${className}`}
      xmlns="http://www.w3.org/2000/svg"
      fill="none"
      viewBox="0 0 24 24"
    >
      <circle
        className="opacity-25"
        cx="12"
        cy="12"
        r="10"
        stroke="currentColor"
        strokeWidth="4"
      ></circle>
      <path
        className="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
      ></path>
    </svg>
  );
}
