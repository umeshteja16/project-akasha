import React, { useState } from "react";
import { useNavigate, useLocation, Link } from "react-router";
import { useAuthStore } from "../../store/auth.store";
import { useUIStore } from "../../store/ui.store";
import { useUser } from "../../hooks/useUser";
import { api } from "../../lib/api";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { LayoutDashboard, LogOut, Search, Sun, Moon, Settings, Menu, X, Plus, Trash2, History, MessageSquare } from "lucide-react";
import { toast } from "sonner";
import { Dialog } from "../ui/Dialog";
import { CommandPalette } from "./CommandPalette";
import { useEffect } from "react";

interface AppLayoutProps {
  children: React.ReactNode;
}

export function AppLayout({ children }: AppLayoutProps) {
  const { clearAuth } = useAuthStore();
  const { theme, setTheme, draggedFileId, setDraggedFileId } = useUIStore();
  const { data: user, isLoading } = useUser();
  const navigate = useNavigate();
  const location = useLocation();
  const [isMobileOpen, setIsMobileOpen] = useState(false);
  const [activeDropColId, setActiveDropColId] = useState<string | null>(null);

  // Collections state
  const queryClient = useQueryClient();
  const [isCreatingCollection, setIsCreatingCollection] = useState(false);
  const [newCollectionName, setNewCollectionName] = useState("");
  const [deleteColDialog, setDeleteColDialog] = useState<{ isOpen: boolean; colId: string; colName: string }>({
    isOpen: false,
    colId: "",
    colName: "",
  });
  const [isCmdPaletteOpen, setIsCmdPaletteOpen] = useState(false);

  // Global Command+K / Ctrl+K keyboard listener
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setIsCmdPaletteOpen((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  // Sync createCollection query parameters to trigger side creator dynamically
  useEffect(() => {
    const params = new URLSearchParams(location.search);
    if (params.get("createCollection") === "true") {
      setIsCreatingCollection(true);
      // Clean query parameter from URL to prevent infinite toggles
      const cleanParams = new URLSearchParams(location.search);
      cleanParams.delete("createCollection");
      const cleanPath = location.pathname + (cleanParams.toString() ? `?${cleanParams.toString()}` : "");
      navigate(cleanPath, { replace: true });
    }
  }, [location.search, navigate]);


  // Fetch user collections dynamically for stark folder listings in sidebar
  const { data: collectionsData } = useQuery<{ data: Array<{ id: string; name: string; color: string }> }>({
    queryKey: ["collections"],
    queryFn: () => api.get("/api/v1/collections"),
  });
  const collectionsList = collectionsData?.data || [];

  // Fetch system health check for offline sovereignty indicator
  const { data: healthData } = useQuery<{ status: string; strictOffline?: boolean }>({
    queryKey: ["health"],
    queryFn: () => api.get("/api/v1/health"),
    refetchInterval: 60000,
  });
  const isStrictOffline = healthData?.strictOffline ?? false;

  const { mutate: createCollection, isPending: isCreating } = useMutation({
    mutationFn: async (payload: { name: string; color: string }) => {
      await api.post("/api/v1/collections", payload);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["collections"] });
      setNewCollectionName("");
      setIsCreatingCollection(false);
      toast.success("Collection created.");
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to create collection.");
    },
  });

  const { mutate: deleteCollection } = useMutation({
    mutationFn: async (colId: string) => {
      await api.delete(`/api/v1/collections/${colId}`);
    },
    onSuccess: (_, colId) => {
      queryClient.invalidateQueries({ queryKey: ["collections"] });
      toast.success("Collection deleted.");
      // If currently inside the collection, redirect to base dashboard
      if (location.pathname === `/collections/${colId}` || location.search.includes(`collectionId=${colId}`)) {
        navigate("/dashboard");
      }
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to delete collection.");
    },
  });

  const handleMoveFileToCollection = async (fileId: string, colId: string) => {
    try {
      await api.patch(`/api/v1/files/${fileId}`, { collectionId: colId });
      toast.success("File organized successfully.");
      queryClient.invalidateQueries({ queryKey: ["files"] });
    } catch (err: any) {
      toast.error(err.message || "Failed to organize file.");
    }
  };

  const handleCreateCollection = (e: React.FormEvent) => {
    e.preventDefault();
    if (!newCollectionName.trim()) return;
    createCollection({ name: newCollectionName.trim(), color: "#8b5cf6" });
  };

  const handleLogout = async () => {
    try {
      await api.post("/api/v1/auth/logout");
    } catch {
      // Ignore failure, proceed with local logout
    } finally {
      clearAuth();
      navigate("/login");
    }
  };

  const navItems = [
    { label: "Dashboard", path: "/dashboard", icon: LayoutDashboard },
    { label: "Search", path: "/search", icon: Search },
    { label: "Chat", path: "/chat", icon: MessageSquare },
    { label: "Activity", path: "/activity", icon: History },
    { label: "Settings", path: "/settings", icon: Settings },
  ];

  return (
    <div className="min-h-screen bg-bg-base flex text-text-primary overflow-hidden font-sans relative">
      {/* Skip-to-content accessibility anchor */}
      <a 
        href="#main-content" 
        className="sr-only focus:not-sr-only focus:fixed focus:top-4 focus:left-4 focus:z-50 focus:px-4 focus:py-2 focus:bg-bg-surface focus:border focus:border-border-strong focus:rounded-lg focus:text-xs focus:text-text-primary font-bold uppercase tracking-wider shadow-xl"
      >
        Skip to content
      </a>

      {/* Mobile Backdrop Overlay */}
      {isMobileOpen && (
        <div
          onClick={() => setIsMobileOpen(false)}
          className="fixed inset-0 bg-black/50 z-30 md:hidden backdrop-blur-sm animate-fade-in"
        />
      )}

      {/* 1. Sidebar */}
      <aside
        className={`fixed inset-y-0 left-0 z-40 w-60 border-r border-border-default bg-bg-surface flex flex-col justify-between select-none transform md:relative md:translate-x-0 transition-transform duration-300 ease-motion ${
          isMobileOpen ? "translate-x-0" : "-translate-x-full md:translate-x-0"
        }`}
      >
        <div>
          {/* Logo Brand Frame - Clickable to Landing Page */}
          <div className="px-6 py-8 border-b border-border-default flex items-center justify-between">
            <Link
              to="/"
              onClick={() => setIsMobileOpen(false)}
              className="inline-flex flex-col items-start gap-1 p-1.5 select-none hover:opacity-85 transition-opacity cursor-pointer"
            >
              <div className="h-[2px] w-[24px] bg-text-primary rounded-full"></div>
              <div className="h-[2px] w-[18px] bg-text-primary rounded-full"></div>
              <div className="h-[2px] w-[12px] bg-text-primary rounded-full"></div>
              <h1 className="text-sm font-semibold tracking-[0.15em] uppercase text-text-primary mt-3 animate-fade-in">
                AKASHA
              </h1>
            </Link>
            
            {/* Close button on mobile sidebar */}
            <button
              onClick={() => setIsMobileOpen(false)}
              className="md:hidden p-1.5 text-text-muted hover:text-text-primary hover:bg-bg-surface-raised border border-border-default rounded-lg transition-all cursor-pointer"
            >
              <X className="h-4.5 w-4.5" />
            </button>
          </div>

          {/* Navigation Links */}
          <nav className="px-4 py-6 flex flex-col gap-1.5">
            {navItems.map((item) => {
              const isActive = location.pathname === item.path;
              const Icon = item.icon;
              return (
                <Link
                  key={item.path}
                  to={item.path}
                  onClick={() => setIsMobileOpen(false)}
                  className={`flex items-center gap-3 px-4 py-3 rounded-lg text-xs font-semibold transition-all relative group border border-l-2 ${
                    isActive
                      ? "bg-accent-subtle text-accent-primary border-accent-subtle-border border-l-accent-primary"
                      : "text-text-muted hover:text-text-primary hover:bg-bg-surface-raised border-transparent border-l-transparent"
                  }`}
                >
                  <Icon className={`h-4.5 w-4.5 ${isActive ? "text-accent-primary" : "text-text-muted group-hover:text-text-primary transition-colors"}`} />
                  <span className={`animated-link-underline pb-0.5 ${isActive ? "text-accent-primary font-bold" : ""}`}>
                    {item.label}
                  </span>
                </Link>
              );
            })}
          </nav>

          {/* Collections Folders Section */}
          <div className="px-4 mt-6 space-y-2 select-none border-t border-border-default/40 pt-4">
            <div className="flex items-center justify-between px-2">
              <span className="text-[10px] uppercase font-bold tracking-widest text-text-dim">Collections</span>
              <button 
                onClick={() => setIsCreatingCollection(true)}
                className="p-1 rounded text-text-dim hover:text-text-primary hover:bg-bg-surface-raised transition-all cursor-pointer"
                title="Create Collection"
              >
                <Plus className="h-3.5 w-3.5" />
              </button>
            </div>

            {/* Inline Creation Form */}
            {isCreatingCollection && (
              <form onSubmit={handleCreateCollection} className="px-2 py-1.5 border border-border-default rounded-lg bg-bg-base/30 animate-fade-in">
                <input
                  type="text"
                  placeholder="Folder name (Enter to save)..."
                  value={newCollectionName}
                  onChange={(e) => setNewCollectionName(e.target.value)}
                  className="w-full bg-bg-surface border border-border-default focus:border-accent-primary focus:outline-none rounded px-2 py-1.5 text-xs text-text-primary font-sans"
                  autoFocus
                  required
                />
                <div className="flex items-center justify-end gap-1.5 mt-1.5">
                  <button
                    type="button"
                    onClick={() => {
                      setIsCreatingCollection(false);
                      setNewCollectionName("");
                    }}
                    className="px-1.5 py-0.5 text-[9px] uppercase font-bold text-text-dim hover:text-text-primary transition-colors cursor-pointer"
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    disabled={isCreating}
                    className="px-2 py-0.5 text-[9px] uppercase font-bold text-accent-primary bg-bg-surface-raised border border-border-default rounded hover:bg-bg-surface-hover transition-all cursor-pointer disabled:opacity-50"
                  >
                    Add
                  </button>
                </div>
              </form>
            )}

            <div className="flex flex-col gap-1.5 max-h-[180px] overflow-y-auto pr-1">
              {collectionsList.map((col) => {
                const isActive = location.pathname === `/collections/${col.id}`;
                return (
                  <div
                    key={col.id}
                    className="relative group/col-item flex items-center w-full"
                    onDragOver={(e) => {
                      e.preventDefault();
                      if (draggedFileId) {
                        e.dataTransfer.dropEffect = "move";
                        setActiveDropColId(col.id);
                      }
                    }}
                    onDragLeave={() => {
                      setActiveDropColId(null);
                    }}
                    onDrop={async (e) => {
                      e.preventDefault();
                      setActiveDropColId(null);
                      if (draggedFileId) {
                        await handleMoveFileToCollection(draggedFileId, col.id);
                        setDraggedFileId(null);
                      }
                    }}
                  >
                    <Link
                      to={`/collections/${col.id}`}
                      onClick={() => setIsMobileOpen(false)}
                      className={`flex items-center gap-3 px-4 py-2 border border-l-2 rounded-lg text-xs font-medium transition-all relative flex-1 group duration-[120ms] ease-motion ${
                        activeDropColId === col.id
                          ? "bg-accent-primary/20 border-accent-primary scale-[1.03] text-accent-primary"
                          : isActive
                          ? "bg-accent-subtle text-accent-primary border-accent-subtle-border border-l-accent-primary"
                          : "text-text-muted hover:text-text-primary hover:bg-bg-surface-raised border-transparent border-l-transparent"
                      }`}
                    >
                      <span 
                        className="h-2 w-2 rounded-full shrink-0 animate-scale-in" 
                        style={{ backgroundColor: col.color }}
                      />
                      <span className="truncate animated-link-underline pb-0.5" title={col.name}>
                        {col.name}
                      </span>
                    </Link>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        e.preventDefault();
                        setDeleteColDialog({
                          isOpen: true,
                          colId: col.id,
                          colName: col.name,
                        });
                      }}
                      className="absolute right-2 opacity-0 group-hover/col-item:opacity-100 text-text-dim hover:text-state-error p-1 transition-opacity cursor-pointer z-10"
                      title="Delete Collection"
                    >
                      <Trash2 className="h-3 w-3" />
                    </button>
                  </div>
                );
              })}
              {collectionsList.length === 0 && !isCreatingCollection && (
                <span className="text-[10px] text-text-dim italic px-2">No folders created.</span>
              )}
            </div>
          </div>
        </div>

        {/* Footer Profile Actions */}
        <div className="flex flex-col gap-4">
          <div className="p-4 border-t border-border-default flex flex-col gap-3">
            {/* User Info Bar */}
            <Link 
              to="/profile"
              onClick={() => setIsMobileOpen(false)}
              className="flex items-center gap-3 p-2 bg-bg-surface-raised hover:bg-bg-surface-hover hover:border-border-strong rounded-lg border border-border-default transition-all duration-motion cursor-pointer"
            >
              <div className="h-8 w-8 rounded-full bg-accent-subtle border border-accent-subtle-border flex items-center justify-center shrink-0 text-accent-primary font-bold text-xs uppercase select-none">
                {isLoading ? "..." : (user?.displayName ? user.displayName.split(" ").map((n: string) => n[0]).join("").toUpperCase().slice(0, 2) : user?.email?.[0]?.toUpperCase() || "U")}
              </div>
              <div className="min-w-0 w-full font-sans">
                <p className="text-xs font-semibold text-text-primary truncate">
                  {isLoading ? "Loading..." : user?.displayName || user?.email?.split("@")[0] || "User"}
                </p>
                <p className="text-[10px] text-text-muted truncate mt-0.5 opacity-80 font-mono">
                  {isLoading ? "Loading..." : user?.email || ""}
                </p>
              </div>
            </Link>

            {/* Logout Action */}
            <button
              onClick={handleLogout}
              className="w-full flex items-center gap-3 px-4 py-3 text-xs font-semibold text-state-error hover:bg-state-error/10 hover:border-state-error/25 border border-transparent rounded-lg transition-all cursor-pointer"
            >
              <LogOut className="h-4.5 w-4.5" />
              Sign Out
            </button>
          </div>
        </div>
      </aside>

      {/* 2. Main Work Panel */}
      <div className="flex-1 flex flex-col min-w-0 h-screen overflow-hidden">
        {/* Top Navbar */}
        <header className="h-16 border-b border-border-default bg-bg-surface flex items-center justify-between px-6 sm:px-8 select-none shrink-0">
          <div className="flex items-center gap-3">
            {/* Mobile Hamburger menu */}
            <button
              onClick={() => setIsMobileOpen(!isMobileOpen)}
              className="md:hidden p-2 text-text-muted hover:text-text-primary hover:bg-bg-surface-raised rounded-lg border border-border-default transition-all cursor-pointer"
            >
              <Menu className="h-4.5 w-4.5" />
            </button>
            
            <h2 className="text-sm font-semibold tracking-wider uppercase text-text-primary">
              {location.pathname === "/dashboard" ? "Dashboard" : location.pathname === "/search" ? "Search" : location.pathname === "/chat" ? "Grounded Chat" : location.pathname === "/settings" ? "Settings" : location.pathname === "/activity" ? "Activity Log" : location.pathname.startsWith("/collections/") ? "Collection Space" : "Workspace"}
            </h2>
          </div>
          <div className="flex items-center gap-3">
            {isStrictOffline && (
              <div className="inline-flex items-center gap-1.5 px-2.5 py-1 bg-state-success/10 border border-state-success/20 text-state-success text-[10px] font-bold uppercase tracking-wider select-none rounded-lg animate-scale">
                <span className="h-1.5 w-1.5 rounded-full bg-state-success animate-pulse"></span>
                Strict Offline Mode
              </div>
            )}
            {/* Dedicated Theme Toggle Button */}
            <button
              onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
              className="p-2 rounded-lg border border-border-default hover:bg-bg-surface-raised text-text-muted hover:text-text-primary transition-all cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-accent-primary flex items-center justify-center"
              title={`Switch to ${theme === "dark" ? "Light" : "Dark"} Theme`}
              aria-label="Toggle Theme"
            >
              {theme === "dark" ? (
                <Sun className="h-4.5 w-4.5" />
              ) : (
                <Moon className="h-4.5 w-4.5" />
              )}
            </button>
          </div>
        </header>

        {/* Scrollable Layout Context Area */}
        <main id="main-content" className="flex-1 overflow-y-auto p-4 sm:p-8 bg-bg-base">
          <div className="max-w-6xl mx-auto">{children}</div>
        </main>
      </div>

      {/* Styled Dialog for Collection Deletion */}
      <Dialog
        isOpen={deleteColDialog.isOpen}
        title="Delete this collection?"
        description={`"${deleteColDialog.colName}" will be permanently deleted. This cannot be undone.`}
        confirmText="Delete"
        cancelText="Cancel"
        onCancel={() => setDeleteColDialog({ isOpen: false, colId: "", colName: "" })}
        onConfirm={() => {
          deleteCollection(deleteColDialog.colId);
          setDeleteColDialog({ isOpen: false, colId: "", colName: "" });
        }}
      />

      {/* Command Palette */}
      <CommandPalette isOpen={isCmdPaletteOpen} onClose={() => setIsCmdPaletteOpen(false)} />
    </div>
  );
}
