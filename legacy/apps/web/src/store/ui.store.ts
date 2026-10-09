import { create } from "zustand";

export interface UploadItem {
  id: string;
  name: string;
  progress: number;
  status: "uploading" | "completed" | "failed";
  error?: string;
}

export interface UIState {
  sidebarOpen: boolean;
  selectedFileId: string | null;
  selectedFileName: string;
  uploadProgress: number | null;
  activeUploads: UploadItem[];
  searchFilters: {
    type: "all" | "pdf" | "image" | "text" | "media";
    mode: "keyword" | "semantic" | "hybrid";
    timeRange: "all" | "day" | "week" | "month";
  };
  theme: "dark" | "light";
  setSidebarOpen: (open: boolean) => void;
  setSelectedFile: (id: string | null, name?: string) => void;
  setUploadProgress: (progress: number | null) => void;
  addActiveUpload: (item: UploadItem) => void;
  updateActiveUpload: (id: string, updates: Partial<UploadItem>) => void;
  clearActiveUploads: () => void;
  setSearchFilters: (filters: Partial<UIState["searchFilters"]>) => void;
  resetFilters: () => void;
  setTheme: (theme: "dark" | "light") => void;
  draggedFileId: string | null;
  setDraggedFileId: (id: string | null) => void;
}

// Initial visual theme load on store registration
const getInitialTheme = (): "dark" | "light" => {
  const saved = localStorage.getItem("theme") as "dark" | "light" | null;
  if (saved === "dark" || saved === "light") {
    return saved;
  }
  return "dark";
};

const initialTheme = getInitialTheme();
document.documentElement.setAttribute("data-theme", initialTheme);
document.documentElement.style.colorScheme = initialTheme;

export const useUIStore = create<UIState>((set) => ({
  sidebarOpen: false,
  selectedFileId: null,
  selectedFileName: "",
  uploadProgress: null,
  activeUploads: [],
  searchFilters: (() => {
    try {
      const saved = localStorage.getItem("search_filters");
      if (saved) {
        return {
          type: "all",
          mode: "hybrid",
          timeRange: "all",
          ...JSON.parse(saved)
        };
      }
    } catch {}
    return {
      type: "all",
      mode: "hybrid",
      timeRange: "all",
    };
  })(),
  theme: initialTheme,
  draggedFileId: null,
  setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
  setSelectedFile: (selectedFileId, selectedFileName = "") =>
    set({ selectedFileId, selectedFileName }),
  setUploadProgress: (uploadProgress) => set({ uploadProgress }),
  addActiveUpload: (item) =>
    set((state) => ({ activeUploads: [...state.activeUploads, item] })),
  updateActiveUpload: (id, updates) =>
    set((state) => ({
      activeUploads: state.activeUploads.map((item) =>
        item.id === id ? { ...item, ...updates } : item
      ),
    })),
  clearActiveUploads: () => set({ activeUploads: [] }),
  setSearchFilters: (filters) =>
    set((state) => {
      const next = { ...state.searchFilters, ...filters };
      localStorage.setItem("search_filters", JSON.stringify(next));
      return { searchFilters: next };
    }),
  resetFilters: () => {
    const next = {
      type: "all" as const,
      mode: "hybrid" as const,
      timeRange: "all" as const,
    };
    localStorage.setItem("search_filters", JSON.stringify(next));
    set({ searchFilters: next });
  },
  setTheme: (theme) => {
    localStorage.setItem("theme", theme);
    document.documentElement.setAttribute("data-theme", theme);
    document.documentElement.style.colorScheme = theme;
    set({ theme });
  },
  setDraggedFileId: (draggedFileId) => set({ draggedFileId }),
}));
