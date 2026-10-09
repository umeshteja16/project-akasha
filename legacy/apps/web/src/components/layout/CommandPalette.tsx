import { useState, useEffect, useRef } from "react";
import { useNavigate } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { FileItem } from "../files/FileCard";
import { Search, FolderPlus, Compass, FileText, X } from "lucide-react";

interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
}

export function CommandPalette({ isOpen, onClose }: CommandPaletteProps) {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Fetch files from TanStack React Query cache / API
  const { data: filesData } = useQuery<{ data: FileItem[] }>({
    queryKey: ["files"],
    queryFn: () => api.get("/api/v1/files"),
    enabled: isOpen,
  });
  const filesList = filesData?.data || [];

  // Real-time inline semantic search directly inside the command palette
  const { data: inlineSearchData, isLoading: isSearching } = useQuery<{
    data: { results: Array<{ fileId: string; fileName: string }> };
  }>({
    queryKey: ["command-palette-search", query],
    queryFn: () => api.get(`/api/v1/search?q=${encodeURIComponent(query)}&mode=semantic&limit=3`),
    enabled: isOpen && query.trim().length > 2,
  });
  const searchResults = inlineSearchData?.data?.results || [];

  // Focus input when opened
  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  // Global meta+k / ctrl+k keydown listener is handled in AppLayout.tsx

  // Internal palette keyboard navigation
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev + 1) % filteredItems.length);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev - 1 + filteredItems.length) % filteredItems.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        const activeItem = filteredItems[selectedIndex];
        if (activeItem) {
          activeItem.action();
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, selectedIndex, query, filesList, searchResults]);

  // Command palette options definition
  const staticNavigation = [
    { label: "Go to Dashboard", category: "Navigate", icon: <Compass className="h-4 w-4" />, action: () => { navigate("/dashboard"); onClose(); } },
    { label: "Go to Semantic Search", category: "Navigate", icon: <Search className="h-4 w-4" />, action: () => { navigate("/search"); onClose(); } },
    { label: "Go to Settings", category: "Navigate", icon: <Compass className="h-4 w-4" />, action: () => { navigate("/settings"); onClose(); } },
    { label: "Go to Activity Timeline", category: "Navigate", icon: <Compass className="h-4 w-4" />, action: () => { navigate("/activity"); onClose(); } },
  ];

  // Dynamic files matching user input
  const filteredFiles = filesList
    .filter((f) => f.originalName.toLowerCase().includes(query.toLowerCase()))
    .slice(0, 5)
    .map((f) => ({
      label: f.originalName,
      category: "Files",
      icon: <FileText className="h-4 w-4 text-text-muted" />,
      action: () => {
        navigate(`/dashboard?openFileId=${f.id}`);
        onClose();
      },
    }));

  // Map real-time inline semantic search results
  const inlineResultsMapped = searchResults.map((r) => ({
    label: r.fileName,
    category: "Search Results",
    icon: <Search className="h-4 w-4 text-accent-primary animate-pulse" />,
    action: () => {
      navigate(`/dashboard?openFileId=${r.fileId}`);
      onClose();
    },
  }));

  const staticActions = [
    { label: "Create a new folder / collection", category: "Actions", icon: <FolderPlus className="h-4 w-4" />, action: () => { navigate("/dashboard?createCollection=true"); onClose(); } },
  ];

  const filteredItems = [
    ...inlineResultsMapped,
    ...filteredFiles,
    ...staticNavigation.filter((item) => item.label.toLowerCase().includes(query.toLowerCase())),
    ...staticActions.filter((item) => item.label.toLowerCase().includes(query.toLowerCase())),
  ];

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-24 px-4 select-none animate-fade-in">
      {/* Backdrop overlay */}
      <div className="absolute inset-0 bg-black/65 backdrop-blur-sm" onClick={onClose} />

      {/* Floating Center Card Frame */}
      <div className="relative bg-bg-surface border border-border-strong rounded-2xl max-w-lg w-full shadow-2xl flex flex-col overflow-hidden max-h-[420px] animate-scale-in">
        {/* Top Search input frame */}
        <div className="flex items-center gap-3 px-4 py-3.5 border-b border-border-default">
          <Search className="h-5 w-5 text-text-muted shrink-0" />
          <input
            ref={inputRef}
            type="text"
            placeholder="Type a command, file, or search query..."
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedIndex(0);
            }}
            className="w-full bg-transparent focus:outline-none text-sm text-text-primary placeholder:text-text-dim"
          />
          {isSearching && (
            <span className="text-[10px] text-accent-primary animate-pulse shrink-0">Searching...</span>
          )}
          <button onClick={onClose} className="p-1 rounded text-text-dim hover:text-text-primary hover:bg-bg-surface-raised transition-all cursor-pointer">
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Results listing */}
        <div className="flex-1 overflow-y-auto p-2.5">
          {filteredItems.length > 0 ? (
            <div className="space-y-4">
              {/* Group items by category */}
              {Array.from(new Set(filteredItems.map((i) => i.category))).map((category) => {
                const catItems = filteredItems.filter((i) => i.category === category);
                return (
                  <div key={category} className="space-y-1.5">
                    <div className="text-xs font-semibold tracking-wide text-text-dim px-3 py-1.5 select-none uppercase">
                      {category}
                    </div>
                    {catItems.map((item) => {
                      const absoluteIndex = filteredItems.indexOf(item);
                      const isActive = absoluteIndex === selectedIndex;
                      return (
                        <div
                          key={item.label + "-" + item.category}
                          onClick={item.action}
                          onMouseEnter={() => setSelectedIndex(absoluteIndex)}
                          className={`flex items-center gap-3 px-3 py-2.5 rounded-lg text-xs font-semibold cursor-pointer select-none transition-all duration-[120ms] border border-transparent ${
                            isActive
                              ? "bg-accent-subtle text-accent-primary border-accent-subtle-border"
                              : "text-text-muted hover:text-text-primary hover:bg-bg-surface-raised"
                          }`}
                        >
                          <span className={`${isActive ? "text-accent-primary" : "text-text-muted"}`}>{item.icon}</span>
                          <span className="truncate flex-1">{item.label}</span>
                          {isActive && (
                            <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-base rounded text-[10px] text-text-dim">↵</kbd>
                          )}
                        </div>
                      );
                    })}
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="py-12 text-center text-xs text-text-dim italic">
              No matching files or commands found.
            </div>
          )}
        </div>

        {/* Footer shortcuts helper */}
        <div className="border-t border-border-default bg-bg-base/40 px-4 py-2.5 flex items-center justify-between text-[10px] text-text-dim select-none font-sans">
          <div className="flex items-center gap-1.5">
            <span>Navigation:</span>
            <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-surface rounded text-[9px]">↑↓</kbd>
            <span>Select:</span>
            <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-surface rounded text-[9px]">Enter</kbd>
            <span>Close:</span>
            <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-surface rounded text-[9px]">Esc</kbd>
          </div>
          <div>Command Palette</div>
        </div>
      </div>
    </div>
  );
}
