import { useState, useEffect } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { useAuthStore } from "../../store/auth.store";
import { useNavigate } from "react-router";
import { useUser } from "../../hooks/useUser";
import { 
  Eye, 
  Search, 
  Cpu, 
  Keyboard, 
  ShieldAlert, 
  Loader2, 
  HardDrive, 
  KeyRound, 
  Sun, 
  Moon
} from "lucide-react";
import { ErrorBoundary } from "../../components/ui/ErrorBoundary";
import { Dialog } from "../../components/ui/Dialog";
import { toast } from "sonner";
import { formatBytes } from "../../components/files/FileCard";

interface UserUsage {
  userCreatedAt: string;
  fileCount: number;
  storageUsed: number;
  storageLimit: number;
  chunkCount: number;
  diskFreeSpace?: number;
}

export function SettingsPage() {
  const { data: user, isLoading: isUserLoading } = useUser();
  const { clearAuth } = useAuthStore();
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  // Active tab state
  const [activeTab, setActiveTab] = useState<"appearance" | "search" | "indexing" | "shortcuts" | "account" | "danger">("appearance");

  // Load user usage
  const { data: usageData, isLoading: isUsageLoading } = useQuery<{ data: UserUsage }>({
    queryKey: ["user-usage"],
    queryFn: () => api.get("/api/v1/user/usage"),
  });
  const usage = usageData?.data;

  // 1. APPEARANCE STATE
  const [themeSetting, setThemeSetting] = useState(() => localStorage.getItem("theme") || "dark");
  const [fontSizeSetting, setFontSizeSetting] = useState(() => localStorage.getItem("font_size") || "default");
  const [animationsSetting, setAnimationsSetting] = useState(() => localStorage.getItem("animations") || "on");
  const [canvasSetting, setCanvasSetting] = useState(() => localStorage.getItem("canvas_bg") || "on");
  const [sidebarWidthSetting, setSidebarWidthSetting] = useState(() => localStorage.getItem("sidebar_width") || "default");

  // 2. SEARCH STATE
  const [defaultSearchMode, setDefaultSearchMode] = useState(() => localStorage.getItem("default_search_mode") || "hybrid");
  const [resultsPerPage, setResultsPerPage] = useState(() => parseInt(localStorage.getItem("results_per_page") || "10", 10));
  const [autoExpandAI, setAutoExpandAI] = useState(() => localStorage.getItem("auto_expand_ai") || "on");

  // 3. INDEXING STATE
  const [autoTagging, setAutoTagging] = useState(() => localStorage.getItem("auto_tagging") || "on");
  const [autoSummary, setAutoSummary] = useState(() => localStorage.getItem("auto_summary") || "on");
  const [ocrLanguage, setOcrLanguage] = useState(() => localStorage.getItem("ocr_language") || "eng");
  const [chunkSizePref, setChunkSizePref] = useState(() => localStorage.getItem("chunk_size_pref") || "medium");

  // 4. CREDENTIALS & ACCOUNT STATE
  const [editName, setEditName] = useState("");
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [customLimitMB, setCustomLimitMB] = useState<number | "">("");
  const [limitUnit, setLimitUnit] = useState<"MB" | "GB">("MB");

  // Dialog States
  const [isDeleteOpen, setIsDeleteOpen] = useState(false);
  const [deleteConfirmText, setDeleteConfirmText] = useState("");
  const [confirmResetOpen, setConfirmResetOpen] = useState(false);

  useEffect(() => {
    if (user?.displayName) {
      setEditName(user.displayName);
    }
  }, [user]);

  // Apply Appearance settings dynamically
  useEffect(() => {
    localStorage.setItem("theme", themeSetting);
    document.documentElement.setAttribute("data-theme", themeSetting);
    document.documentElement.style.colorScheme = themeSetting;
  }, [themeSetting]);

  useEffect(() => {
    localStorage.setItem("font_size", fontSizeSetting);
    if (fontSizeSetting === "compact") {
      document.documentElement.style.fontSize = "13px";
    } else if (fontSizeSetting === "default") {
      document.documentElement.style.fontSize = "14.4px";
    } else {
      document.documentElement.style.fontSize = "16px";
    }
  }, [fontSizeSetting]);

  useEffect(() => {
    localStorage.setItem("animations", animationsSetting);
  }, [animationsSetting]);

  useEffect(() => {
    localStorage.setItem("canvas_bg", canvasSetting);
  }, [canvasSetting]);

  useEffect(() => {
    localStorage.setItem("sidebar_width", sidebarWidthSetting);
  }, [sidebarWidthSetting]);

  // Apply Search Settings
  useEffect(() => {
    localStorage.setItem("default_search_mode", defaultSearchMode);
  }, [defaultSearchMode]);

  useEffect(() => {
    localStorage.setItem("results_per_page", resultsPerPage.toString());
  }, [resultsPerPage]);

  useEffect(() => {
    localStorage.setItem("auto_expand_ai", autoExpandAI);
  }, [autoExpandAI]);

  // Apply Indexing preferences
  useEffect(() => {
    localStorage.setItem("auto_tagging", autoTagging);
  }, [autoTagging]);

  useEffect(() => {
    localStorage.setItem("auto_summary", autoSummary);
  }, [autoSummary]);

  useEffect(() => {
    localStorage.setItem("ocr_language", ocrLanguage);
  }, [ocrLanguage]);

  useEffect(() => {
    localStorage.setItem("chunk_size_pref", chunkSizePref);
  }, [chunkSizePref]);

  // Mutations
  const { mutate: updateProfile, isPending: isProfilePending } = useMutation({
    mutationFn: async (newName: string) => {
      return api.patch("/api/v1/me", { displayName: newName });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["user"] });
      toast.success("Profile displayName updated.");
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to update profile.");
    },
  });

  const { mutate: updateStorageLimit, isPending: isLimitPending } = useMutation({
    mutationFn: async (limitMB: number) => {
      return api.patch("/api/v1/user/storage-limit", { limitMB });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["user-usage"] });
      toast.success("Storage limit customized successfully.");
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to update storage limit.");
    },
  });

  const { mutate: changePassword, isPending: isPasswordPending } = useMutation({
    mutationFn: async () => {
      if (newPassword !== confirmPassword) {
        throw new Error("New passwords do not match.");
      }
      if (newPassword.length < 8) {
        throw new Error("New password must be at least 8 characters long.");
      }
      return api.post("/api/v1/user/change-password", {
        currentPassword,
        newPassword,
      });
    },
    onSuccess: () => {
      toast.success("Password updated successfully.");
      setCurrentPassword("");
      setNewPassword("");
      setConfirmPassword("");
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to update password. Verify current credentials.");
    },
  });

  const { mutate: deleteAccount, isPending: isDeletePending } = useMutation({
    mutationFn: () => api.delete("/api/v1/user/account"),
    onSuccess: () => {
      clearAuth();
      navigate("/login");
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to delete account.");
    },
  });

  const handlePasswordSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    changePassword();
  };

  const handleClearSearchHistory = () => {
    localStorage.removeItem("search_history");
    toast.success("Local search query history cleared.");
  };

  const handleResetPreferences = () => {
    localStorage.clear();
    setThemeSetting("dark");
    setFontSizeSetting("default");
    setAnimationsSetting("on");
    setCanvasSetting("on");
    setSidebarWidthSetting("default");
    setDefaultSearchMode("hybrid");
    setResultsPerPage(10);
    setAutoExpandAI("on");
    setAutoTagging("on");
    setAutoSummary("on");
    setOcrLanguage("eng");
    setChunkSizePref("medium");
    toast.success("All preferences reset to standard values.");
    setConfirmResetOpen(false);
  };

  const handleExportData = async () => {
    try {
      const filesRes = await api.get("/api/v1/files");
      const list = filesRes?.data || [];
      const dataStr = "data:text/json;charset=utf-8," + encodeURIComponent(JSON.stringify(list, null, 2));
      const link = document.createElement("a");
      link.setAttribute("href", dataStr);
      link.setAttribute("download", `akasha_export_${new Date().toISOString().slice(0, 10)}.json`);
      link.click();
      toast.success("Archive metadata exported successfully.");
    } catch {
      toast.error("Export failed. Server unavailable.");
    }
  };

  const storageLimit = usage?.storageLimit || 50 * 1024 * 1024;
  const storageUsed = usage?.storageUsed || 0;
  const storagePercentage = Math.min(Math.round((storageUsed / storageLimit) * 100), 100);

  const displayVal = customLimitMB === "" 
    ? (limitUnit === "GB" ? Math.round(storageLimit / (1024 * 1024 * 1024)) : Math.round(storageLimit / (1024 * 1024)))
    : customLimitMB;

  const isMac = navigator.platform.toUpperCase().indexOf("MAC") >= 0;
  const metaKeyLabel = isMac ? "⌘" : "Ctrl";

  const tabs = [
    { id: "appearance" as const, label: "Appearance", icon: Eye },
    { id: "search" as const, label: "Search", icon: Search },
    { id: "indexing" as const, label: "Indexing", icon: Cpu },
    { id: "shortcuts" as const, label: "Shortcuts", icon: Keyboard },
    { id: "account" as const, label: "Account & Storage", icon: HardDrive },
    { id: "danger" as const, label: "Danger Zone", icon: ShieldAlert },
  ];

  return (
    <ErrorBoundary>
      <div className="space-y-8 select-none font-sans max-w-4xl mx-auto pb-16">
        
        {/* Stark Page Title */}
        <div className="border-b border-border-default pb-6 select-none">
          <span className="text-xs font-semibold uppercase tracking-wider text-accent-primary">
            Settings
          </span>
          <h1 className="font-serif text-4xl font-normal tracking-tight text-text-primary mt-1 select-text selection:bg-accent-primary/20">
            System Preferences.
          </h1>
        </div>

        {/* Tab Navigation pill switcher */}
        <div className="flex flex-wrap gap-2 select-none border-b border-border-default/40 pb-4">
          {tabs.map((t) => {
            const Icon = t.icon;
            return (
              <button
                key={t.id}
                type="button"
                onClick={() => setActiveTab(t.id)}
                className={`flex items-center gap-2 px-4 py-2 text-xs font-semibold rounded-xl border transition-all cursor-pointer ${
                  activeTab === t.id
                    ? "bg-accent-subtle text-accent-primary border-accent-subtle-border"
                    : "text-text-muted hover:text-text-primary hover:bg-bg-surface-raised border-transparent"
                }`}
              >
                <Icon className="h-4 w-4" />
                {t.label}
              </button>
            );
          })}
        </div>

        {/* 1. APPEARANCE TAB PANEL */}
        {activeTab === "appearance" && (
          <div className="bg-bg-surface border border-border-default rounded-2xl p-6 md:p-8 space-y-6 shadow-sm animate-fade-in">
            <h3 className="text-sm font-semibold uppercase tracking-wider text-text-primary">Appearance Preferences</h3>
            <p className="text-xs text-text-muted leading-relaxed">Customize visual density, scaling parameters, theme styles, and animation flows.</p>
            
            <div className="space-y-6 pt-2">
              {/* Theme toggle */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-2 border-b border-border-default/30 pb-4">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Active Visual Theme</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Switch interface color configurations.</span>
                </div>
                <div className="flex gap-1.5 bg-bg-base border border-border-default rounded-xl p-1 select-none w-fit">
                  {["dark", "light"].map((t) => (
                    <button
                      key={t}
                      type="button"
                      onClick={() => setThemeSetting(t)}
                      className={`px-3 py-1.5 text-[10px] font-bold rounded-lg uppercase tracking-wider transition-all cursor-pointer flex items-center gap-1.5 ${themeSetting === t ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"}`}
                    >
                      {t === "dark" ? <Moon className="h-3.5 w-3.5" /> : <Sun className="h-3.5 w-3.5" />}
                      {t}
                    </button>
                  ))}
                </div>
              </div>

              {/* Font Density scaling */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-2 border-b border-border-default/30 pb-4">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Layout Font Scaling Density</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Adjust text scale and layout packing densities.</span>
                </div>
                <div className="flex gap-1.5 bg-bg-base border border-border-default rounded-xl p-1 select-none w-fit">
                  {[
                    { id: "compact", label: "Compact 13px" },
                    { id: "default", label: "Default 14.4px" },
                    { id: "comfortable", label: "Comfortable 16px" }
                  ].map((f) => (
                    <button
                      key={f.id}
                      type="button"
                      onClick={() => setFontSizeSetting(f.id)}
                      className={`px-3 py-1.5 text-[10px] font-bold rounded-lg uppercase tracking-wider transition-all cursor-pointer ${fontSizeSetting === f.id ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"}`}
                    >
                      {f.label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Sidebar Width */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-2 border-b border-border-default/30 pb-4">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Sidebar Layout Width</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Optimize left partition navigation widths.</span>
                </div>
                <div className="flex gap-1.5 bg-bg-base border border-border-default rounded-xl p-1 select-none w-fit">
                  {[
                    { id: "narrow", label: "Narrow" },
                    { id: "default", label: "Default" },
                    { id: "wide", label: "Wide" }
                  ].map((w) => (
                    <button
                      key={w.id}
                      type="button"
                      onClick={() => setSidebarWidthSetting(w.id)}
                      className={`px-3 py-1.5 text-[10px] font-bold rounded-lg uppercase tracking-wider transition-all cursor-pointer ${sidebarWidthSetting === w.id ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"}`}
                    >
                      {w.label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Animations toggle */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-2 border-b border-border-default/30 pb-4">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Visual Motion Transitions</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Toggle interface animations and smooth micro-reveals.</span>
                </div>
                <div className="flex gap-1.5 bg-bg-base border border-border-default rounded-xl p-1 select-none w-fit">
                  {["on", "off"].map((a) => (
                    <button
                      key={a}
                      type="button"
                      onClick={() => setAnimationsSetting(a)}
                      className={`px-3 py-1.5 text-[10px] font-bold rounded-lg uppercase tracking-wider transition-all cursor-pointer ${animationsSetting === a ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"}`}
                    >
                      {a}
                    </button>
                  ))}
                </div>
              </div>

              {/* Canvas background toggle */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-2">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Canvas Graphic Backdrops</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Toggle deep stellar canvas shimmers on empty pages.</span>
                </div>
                <div className="flex gap-1.5 bg-bg-base border border-border-default rounded-xl p-1 select-none w-fit">
                  {["on", "off"].map((c) => (
                    <button
                      key={c}
                      type="button"
                      onClick={() => setCanvasSetting(c)}
                      className={`px-3 py-1.5 text-[10px] font-bold rounded-lg uppercase tracking-wider transition-all cursor-pointer ${canvasSetting === c ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"}`}
                    >
                      {c}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          </div>
        )}

        {/* 2. SEARCH TAB PANEL */}
        {activeTab === "search" && (
          <div className="bg-bg-surface border border-border-default rounded-2xl p-6 md:p-8 space-y-6 shadow-sm animate-fade-in">
            <h3 className="text-sm font-semibold uppercase tracking-wider text-text-primary">Search Configuration</h3>
            <p className="text-xs text-text-muted leading-relaxed">Customize RAG defaults, semantic thresholding models, and diagnostics telemetries.</p>
            
            <div className="space-y-6 pt-2">
              {/* Default Search Mode */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-2 border-b border-border-default/30 pb-4">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Default Search Mode</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Select initial index query processing algorithms.</span>
                </div>
                <div className="flex gap-1.5 bg-bg-base border border-border-default rounded-xl p-1 select-none w-fit font-mono">
                  {["keyword", "semantic", "hybrid"].map((m) => (
                    <button
                      key={m}
                      type="button"
                      onClick={() => setDefaultSearchMode(m)}
                      className={`px-3 py-1.5 text-[10px] font-bold rounded-lg uppercase tracking-wider transition-all cursor-pointer ${defaultSearchMode === m ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"}`}
                    >
                      {m}
                    </button>
                  ))}
                </div>
              </div>

              {/* Results Per Page */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-2 border-b border-border-default/30 pb-4">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Search Results Per Page</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Determine how many matching snippets display concurrently.</span>
                </div>
                <div className="flex gap-1.5 bg-bg-base border border-border-default rounded-xl p-1 select-none w-fit font-mono">
                  {[5, 10, 20, 50].map((r) => (
                    <button
                      key={r}
                      type="button"
                      onClick={() => setResultsPerPage(r)}
                      className={`px-3 py-1.5 text-[10px] font-bold rounded-lg uppercase tracking-wider transition-all cursor-pointer ${resultsPerPage === r ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"}`}
                    >
                      {r}
                    </button>
                  ))}
                </div>
              </div>

              {/* Auto Expand AI */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-2">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Auto-synthesize AI answers</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Run grounded turn synthesizers on search immediately if active.</span>
                </div>
                <div className="flex gap-1.5 bg-bg-base border border-border-default rounded-xl p-1 select-none w-fit">
                  {["on", "off"].map((a) => (
                    <button
                      key={a}
                      type="button"
                      onClick={() => setAutoExpandAI(a)}
                      className={`px-3 py-1.5 text-[10px] font-bold rounded-lg uppercase tracking-wider transition-all cursor-pointer ${autoExpandAI === a ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"}`}
                    >
                      {a}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          </div>
        )}

        {/* 3. INDEXING TAB PANEL */}
        {activeTab === "indexing" && (
          <div className="bg-bg-surface border border-border-default rounded-2xl p-6 md:p-8 space-y-6 shadow-sm animate-fade-in">
            <h3 className="text-sm font-semibold uppercase tracking-wider text-text-primary">Indexing & OCR Preferences</h3>
            <p className="text-xs text-text-muted leading-relaxed">Fine-tune OCR languages, vector passage chunking lengths, and semantic summaries.</p>
            
            <div className="space-y-6 pt-2">
              {/* Chunk Size */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-2 border-b border-border-default/30 pb-4">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Chunk Size Allocations</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Passage length mappings: small (500 chars) / medium (1000) / large (2000).</span>
                </div>
                <div className="flex gap-1.5 bg-bg-base border border-border-default rounded-xl p-1 select-none w-fit font-mono">
                  {["small", "medium", "large"].map((c) => (
                    <button
                      key={c}
                      type="button"
                      onClick={() => setChunkSizePref(c)}
                      className={`px-3 py-1.5 text-[10px] font-bold rounded-lg uppercase tracking-wider transition-all cursor-pointer ${chunkSizePref === c ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"}`}
                    >
                      {c}
                    </button>
                  ))}
                </div>
              </div>

              {/* OCR language */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-2 border-b border-border-default/30 pb-4">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Primary OCR language</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Specify language processor for off-line OCR scans.</span>
                </div>
                <select
                  value={ocrLanguage}
                  onChange={(e) => setOcrLanguage(e.target.value)}
                  className="bg-bg-base border border-border-default text-text-primary text-xs font-semibold rounded-xl p-2.5 outline-none focus:border-accent-primary font-mono select-none"
                >
                  <option value="eng">English (eng)</option>
                  <option value="spa">Spanish (spa)</option>
                  <option value="fra">French (fra)</option>
                  <option value="deu">German (deu)</option>
                  <option value="chi_sim">Chinese Sim. (chi_sim)</option>
                  <option value="jpn">Japanese (jpn)</option>
                </select>
              </div>

              {/* Auto tagging */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-2 border-b border-border-default/30 pb-4">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Automatic Semantic Tagging</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Generate semantic tags during asynchronous pipelines.</span>
                </div>
                <div className="flex gap-1.5 bg-bg-base border border-border-default rounded-xl p-1 select-none w-fit">
                  {["on", "off"].map((a) => (
                    <button
                      key={a}
                      type="button"
                      onClick={() => setAutoTagging(a)}
                      className={`px-3 py-1.5 text-[10px] font-bold rounded-lg uppercase tracking-wider transition-all cursor-pointer ${autoTagging === a ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"}`}
                    >
                      {a}
                    </button>
                  ))}
                </div>
              </div>

              {/* Auto Summary */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-2">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Automatic Document Summaries</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Generate one-liner summaries on successful ingestion.</span>
                </div>
                <div className="flex gap-1.5 bg-bg-base border border-border-default rounded-xl p-1 select-none w-fit">
                  {["on", "off"].map((a) => (
                    <button
                      key={a}
                      type="button"
                      onClick={() => setAutoSummary(a)}
                      className={`px-3 py-1.5 text-[10px] font-bold rounded-lg uppercase tracking-wider transition-all cursor-pointer ${autoSummary === a ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"}`}
                    >
                      {a}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          </div>
        )}

        {/* 4. KEYBOARD SHORTCUTS TAB PANEL */}
        {activeTab === "shortcuts" && (
          <div className="bg-bg-surface border border-border-default rounded-2xl p-6 md:p-8 space-y-6 shadow-sm animate-fade-in">
            <div className="flex items-center gap-2 border-b border-border-default pb-4">
              <Keyboard className="h-5 w-5 text-accent-primary" />
              <h3 className="text-base font-semibold text-text-primary">Keyboard Shortcuts</h3>
            </div>
            
            <div className="grid grid-cols-1 md:grid-cols-2 gap-8 text-xs select-none">
              <div className="space-y-4">
                <h4 className="font-semibold text-text-primary uppercase tracking-wider text-[11px] text-accent-primary">Global Navigation</h4>
                <table className="w-full text-left border-collapse font-sans">
                  <tbody>
                    <tr className="border-b border-border-default/30">
                      <td className="py-2.5 text-text-muted">Command Palette</td>
                      <td className="py-2.5 text-right font-semibold">
                        <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-base rounded text-[10px] text-text-dim font-mono">{metaKeyLabel}+K</kbd>
                      </td>
                    </tr>
                    <tr className="border-b border-border-default/30">
                      <td className="py-2.5 text-text-muted">Open Search Input</td>
                      <td className="py-2.5 text-right font-semibold">
                        <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-base rounded text-[10px] text-text-dim font-mono">/</kbd>
                      </td>
                    </tr>
                    <tr className="border-b border-border-default/30">
                      <td className="py-2.5 text-text-muted">Close active Dialog/Panel</td>
                      <td className="py-2.5 text-right font-semibold">
                        <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-base rounded text-[10px] text-text-dim font-mono">Esc</kbd>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <div className="space-y-4">
                <h4 className="font-semibold text-text-primary uppercase tracking-wider text-[11px] text-accent-primary">Dashboard Controls</h4>
                <table className="w-full text-left border-collapse font-sans">
                  <tbody>
                    <tr className="border-b border-border-default/30">
                      <td className="py-2.5 text-text-muted">Card selection lists</td>
                      <td className="py-2.5 text-right font-semibold">
                        <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-base rounded text-[10px] text-text-dim font-mono">J</kbd> / <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-base rounded text-[10px] text-text-dim font-mono">K</kbd>
                      </td>
                    </tr>
                    <tr className="border-b border-border-default/30">
                      <td className="py-2.5 text-text-muted">Open Selected file</td>
                      <td className="py-2.5 text-right font-semibold">
                        <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-base rounded text-[10px] text-text-dim font-mono">Enter</kbd>
                      </td>
                    </tr>
                    <tr className="border-b border-border-default/30">
                      <td className="py-2.5 text-text-muted">Toggle star / favorite</td>
                      <td className="py-2.5 text-right font-semibold">
                        <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-base rounded text-[10px] text-text-dim font-mono">S</kbd>
                      </td>
                    </tr>
                    <tr className="border-b border-border-default/30">
                      <td className="py-2.5 text-text-muted">F2 Inline Rename</td>
                      <td className="py-2.5 text-right font-semibold">
                        <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-base rounded text-[10px] text-text-dim font-mono">F2</kbd>
                      </td>
                    </tr>
                    <tr className="border-b border-border-default/30">
                      <td className="py-2.5 text-text-muted">Download / Delete File</td>
                      <td className="py-2.5 text-right font-semibold">
                        <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-base rounded text-[10px] text-text-dim font-mono">D</kbd> / <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-base rounded text-[10px] text-text-dim font-mono">Del</kbd>
                      </td>
                    </tr>
                    <tr className="border-b border-border-default/30">
                      <td className="py-2.5 text-text-muted">Select All list cards</td>
                      <td className="py-2.5 text-right font-semibold">
                        <kbd className="px-1.5 py-0.5 border border-border-default bg-bg-base rounded text-[10px] text-text-dim font-mono">{metaKeyLabel}+A</kbd>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>
          </div>
        )}

        {/* 5. CREDENTIALS & ACCOUNT PANEL */}
        {activeTab === "account" && (
          <div className="space-y-8 animate-fade-in">
            {/* Identity display name edit */}
            <div className="bg-bg-surface border border-border-default rounded-2xl p-6 md:p-8 space-y-4 shadow-sm">
              <h3 className="text-sm font-semibold uppercase tracking-wider text-text-primary">Identity Profile</h3>
              <p className="text-xs text-text-muted leading-relaxed">Customize your public credentials and workspace representation.</p>
              
              <div className="flex flex-col sm:flex-row gap-3 max-w-lg items-stretch sm:items-center pt-2">
                <input
                  type="text"
                  value={editName}
                  onChange={(e) => setEditName(e.target.value)}
                  placeholder="Anonymous Cognizant..."
                  className="flex-1 min-w-0 bg-bg-base border border-border-default focus:border-text-primary hover:border-border-strong rounded-xl p-3 text-xs text-text-primary font-semibold transition-all focus:outline-none"
                />
                <button
                  type="button"
                  onClick={() => updateProfile(editName)}
                  disabled={isProfilePending || isUserLoading}
                  className="px-5 py-3 bg-accent-primary hover:bg-accent-hover text-bg-base rounded-xl text-xs font-bold uppercase tracking-wider transition-all disabled:opacity-50 cursor-pointer shadow-sm shrink-0"
                >
                  {isProfilePending ? "Saving..." : "Rename Identity"}
                </button>
              </div>
            </div>

            {/* Custom storage allocation */}
            <div className="bg-bg-surface border border-border-default rounded-2xl p-6 md:p-8 space-y-6 shadow-sm">
              <div className="flex items-center gap-3.5 border-b border-border-default/50 pb-4">
                <div className="p-3 bg-accent-subtle border border-accent-subtle-border rounded-xl text-accent-primary shrink-0">
                  <HardDrive className="h-5 w-5" />
                </div>
                <div>
                  <h3 className="text-base font-semibold text-text-primary">Secure Storage Volume</h3>
                  <p className="text-xs text-text-muted mt-0.5">Summary of physical archive volume details.</p>
                </div>
              </div>

              <div className="space-y-4">
                {/* Progress bar */}
                <div className="space-y-2">
                  <div className="flex justify-between text-xs font-semibold text-text-muted">
                    <span>{storagePercentage}% Consumed</span>
                    <span>{isUsageLoading ? "..." : `${formatBytes(storageUsed)} of ${formatBytes(storageLimit)}`}</span>
                  </div>
                  <div className="h-3 w-full bg-bg-base border border-border-default rounded-full overflow-hidden p-[2px]">
                    <div 
                      className="h-full bg-accent-primary rounded-full transition-all duration-500 ease-out" 
                      style={{ width: `${storagePercentage}%` }}
                    />
                  </div>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <div className="flex items-center justify-between text-xs font-semibold text-text-muted bg-bg-base border border-border-default rounded-xl p-3">
                    <span>Free Space (Disk):</span>
                    <span className="text-text-primary font-mono">{isUsageLoading ? "..." : formatBytes(usage?.diskFreeSpace || 0)}</span>
                  </div>
                  <div className="flex items-center justify-between text-xs font-semibold text-text-muted bg-bg-base border border-border-default rounded-xl p-3">
                    <span>Free Limit Space:</span>
                    <span className="text-text-primary font-mono">{isUsageLoading ? "..." : formatBytes(Math.max(0, storageLimit - storageUsed))}</span>
                  </div>
                </div>

                <div className="pt-4 border-t border-border-default/30 space-y-3.5 select-none">
                  <label className="text-xs font-medium text-text-muted block">Custom Storage Allocation</label>
                  <div className="flex flex-col sm:flex-row gap-3 max-w-lg items-stretch sm:items-center">
                    <input
                      type="number"
                      min="1"
                      max="100000"
                      value={displayVal}
                      onChange={(e) => setCustomLimitMB(e.target.value === "" ? "" : Number(e.target.value))}
                      className="flex-1 min-w-0 bg-bg-base border border-border-default focus:border-text-primary hover:border-border-strong rounded-xl p-3 text-xs text-text-primary font-semibold transition-all focus:outline-none placeholder:text-text-dim"
                      placeholder="e.g. 50"
                    />
                    
                    <div className="flex gap-1 bg-bg-base border border-border-default rounded-xl p-1 shrink-0 select-none items-center">
                      {["MB", "GB"].map((u) => (
                        <button
                          key={u}
                          type="button"
                          onClick={() => {
                            setLimitUnit(u as "MB" | "GB");
                            setCustomLimitMB("");
                          }}
                          className={`px-3 py-1.5 text-[11px] font-bold rounded-lg uppercase tracking-wider transition-all cursor-pointer ${
                            limitUnit === u ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"
                          }`}
                        >
                          {u}
                        </button>
                      ))}
                    </div>

                    <button
                      type="button"
                      onClick={() => {
                        const valMB = limitUnit === "GB" ? Number(displayVal) * 1024 : Number(displayVal);
                        updateStorageLimit(valMB);
                      }}
                      disabled={isLimitPending || isUsageLoading}
                      className="px-5 py-3 bg-accent-primary hover:bg-accent-hover text-bg-base rounded-xl text-xs font-bold uppercase tracking-wider transition-all disabled:opacity-50 cursor-pointer shadow-sm shrink-0"
                    >
                      {isLimitPending ? "Saving..." : "Set Allocation"}
                    </button>
                  </div>
                </div>
              </div>
            </div>

            {/* Change credentials password */}
            <div className="bg-bg-surface border border-border-default rounded-2xl p-6 md:p-8 space-y-6 shadow-sm">
              <div className="flex items-center gap-3.5 border-b border-border-default/50 pb-4">
                <div className="p-3 bg-accent-subtle border border-accent-subtle-border rounded-xl text-accent-primary shrink-0">
                  <KeyRound className="h-5 w-5" />
                </div>
                <div>
                  <h3 className="text-base font-semibold text-text-primary">Credential Update</h3>
                  <p className="text-xs text-text-muted mt-0.5">Safely update your login passphrase turn.</p>
                </div>
              </div>

              <form onSubmit={handlePasswordSubmit} className="space-y-4 max-w-md select-none">
                <div className="space-y-1">
                  <label className="text-xs font-medium text-text-muted block">Current Password</label>
                  <input 
                    type="password" 
                    value={currentPassword}
                    onChange={(e) => setCurrentPassword(e.target.value)}
                    required
                    className="w-full bg-bg-base border border-border-default focus:border-text-primary hover:border-border-strong rounded-xl p-3 text-xs text-text-primary transition-all focus:outline-none"
                    placeholder="••••••••••••"
                  />
                </div>

                <div className="space-y-1">
                  <label className="text-xs font-medium text-text-muted block">New Password</label>
                  <input 
                    type="password" 
                    value={newPassword}
                    onChange={(e) => setNewPassword(e.target.value)}
                    required
                    className="w-full bg-bg-base border border-border-default focus:border-text-primary hover:border-border-strong rounded-xl p-3 text-xs text-text-primary transition-all focus:outline-none"
                    placeholder="••••••••••••"
                  />
                </div>

                <div className="space-y-1">
                  <label className="text-xs font-medium text-text-muted block">Confirm New Password</label>
                  <input 
                    type="password" 
                    value={confirmPassword}
                    onChange={(e) => setConfirmPassword(e.target.value)}
                    required
                    className="w-full bg-bg-base border border-border-default focus:border-text-primary hover:border-border-strong rounded-xl p-3 text-xs text-text-primary transition-all focus:outline-none"
                    placeholder="••••••••••••"
                  />
                </div>

                <button
                  type="submit"
                  disabled={isPasswordPending}
                  className="flex items-center justify-center gap-2 px-5 py-3 bg-accent-primary hover:bg-accent-hover text-bg-base rounded-xl text-xs font-semibold uppercase tracking-wider transition-all disabled:opacity-50 cursor-pointer shadow-sm mt-4 min-w-[140px]"
                >
                  {isPasswordPending ? (
                    <Loader2 className="h-4 w-4 animate-spin text-bg-base" />
                  ) : (
                    "Save Changes"
                  )}
                </button>
              </form>
            </div>
          </div>
        )}

        {/* 6. DANGER ZONE TAB PANEL */}
        {activeTab === "danger" && (
          <div className="bg-bg-surface border border-state-error/25 rounded-2xl p-6 md:p-8 space-y-6 shadow-sm animate-fade-in select-none">
            <div className="flex items-center gap-3.5 border-b border-border-default pb-4">
              <div className="p-3 bg-state-error/10 border border-state-error/20 rounded-xl text-state-error shrink-0 animate-pulse">
                <ShieldAlert className="h-5 w-5" />
              </div>
              <div>
                <h3 className="text-base font-semibold text-state-error">Danger Zone</h3>
                <p className="text-xs text-text-muted mt-0.5">Content-irreversible profile operations.</p>
              </div>
            </div>

            <div className="space-y-6 select-none">
              
              {/* Local preferences reset */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-4 border-b border-border-default/30 pb-4">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Reset preferences</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Revert theme settings and scaling values to standard values.</span>
                </div>
                <button
                  type="button"
                  onClick={() => setConfirmResetOpen(true)}
                  className="px-4 py-2.5 border border-border-default hover:border-border-strong rounded-xl text-xs font-bold uppercase tracking-wider text-text-primary bg-bg-base hover:bg-bg-surface-raised cursor-pointer shadow-sm shrink-0"
                >
                  Reset preferences
                </button>
              </div>

              {/* Data Export */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-4 border-b border-border-default/30 pb-4">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Export metadata structure</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Download full JSON summary profile of all active indexed references.</span>
                </div>
                <button
                  type="button"
                  onClick={handleExportData}
                  className="px-4 py-2.5 border border-border-default hover:border-border-strong rounded-xl text-xs font-bold uppercase tracking-wider text-text-primary bg-bg-base hover:bg-bg-surface-raised cursor-pointer shadow-sm shrink-0"
                >
                  Export Data
                </button>
              </div>

              {/* Clear History */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-4 border-b border-border-default/30 pb-4">
                <div>
                  <span className="text-xs font-semibold text-text-primary block">Erase search query history</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Flush local search entries and bookmarks from browser caches.</span>
                </div>
                <button
                  type="button"
                  onClick={handleClearSearchHistory}
                  className="px-4 py-2.5 border border-border-default hover:border-border-strong rounded-xl text-xs font-bold uppercase tracking-wider text-text-primary bg-bg-base hover:bg-bg-surface-raised cursor-pointer shadow-sm shrink-0"
                >
                  Clear history
                </button>
              </div>

              {/* Delete Account */}
              <div className="flex flex-col sm:flex-row justify-between sm:items-center gap-4">
                <div>
                  <span className="text-xs font-semibold text-state-error block">Erase account & indexed folders</span>
                  <span className="text-[11px] text-text-muted block mt-0.5">Permanently cascade delete user references, files, passages, and tokens.</span>
                </div>
                <button
                  type="button"
                  onClick={() => setIsDeleteOpen(true)}
                  className="px-4 py-2.5 bg-state-error/15 border border-state-error/25 hover:bg-state-error/25 text-state-error rounded-xl text-xs font-bold uppercase tracking-wider cursor-pointer shadow-sm shrink-0"
                >
                  Delete account
                </button>
              </div>
            </div>
          </div>
        )}

        {/* Delete cascade Dialog */}
        <Dialog
          isOpen={isDeleteOpen}
          title="Completely delete account?"
          description="WARNING: This cascade deletion will permanently drop all vector chunks, indexes, conversation messages, and physical files. Type 'DELETE' to confirm."
          cancelText="Cancel"
          confirmText="Irreversibly Delete Account"
          onCancel={() => {
            setIsDeleteOpen(false);
            setDeleteConfirmText("");
          }}
          onConfirm={() => {
            if (deleteConfirmText === "DELETE") {
              deleteAccount();
            } else {
              toast.error("Typing mismatch. Verify delete credentials.");
            }
          }}
          isConfirming={isDeletePending}
          variant="danger"
        >
          <div className="mt-4">
            <input 
              type="text" 
              value={deleteConfirmText}
              onChange={(e) => setDeleteConfirmText(e.target.value)}
              placeholder="Type 'DELETE' here"
              className="w-full bg-bg-base border border-state-error/30 focus:border-state-error rounded-xl p-3 text-xs text-text-primary focus:outline-none uppercase font-mono tracking-widest text-center"
            />
          </div>
        </Dialog>

        {/* Reset settings Dialog */}
        <Dialog
          isOpen={confirmResetOpen}
          title="Reset all preferences?"
          description="All local styling options, search algorithms defaults, indexing sizing parameters, and shortcuts guides will revert to their standard default values."
          cancelText="Cancel"
          confirmText="Confirm Reset"
          onCancel={() => setConfirmResetOpen(false)}
          onConfirm={handleResetPreferences}
          variant="danger"
        />
      </div>
    </ErrorBoundary>
  );
}
