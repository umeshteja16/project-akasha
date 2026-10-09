import { useState, useEffect } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { useUser } from "../../hooks/useUser";
import { useNavigate, Link } from "react-router";
import { 
  Mail, 
  Calendar, 
  FileText, 
  Brain, 
  HardDrive, 
  Folder, 
  Tag, 
  Activity, 
  ChevronRight, 
  Check, 
  Loader2
} from "lucide-react";
import { ErrorBoundary } from "../../components/ui/ErrorBoundary";
import { toast } from "sonner";

interface UserUsage {
  userCreatedAt: string;
  fileCount: number;
  storageUsed: number;
  storageLimit: number;
  chunkCount: number;
}

interface ActivityLog {
  id: string;
  action: string;
  ipAddress: string;
  createdAt: string;
  fileId: string | null;
  fileName: string | null;
}

export function ProfilePage() {
  const { data: user } = useUser();
  const queryClient = useQueryClient();
  const navigate = useNavigate();

  // Inline editing displayName
  const [isEditingName, setIsEditingName] = useState(false);
  const [editName, setEditName] = useState("");

  useEffect(() => {
    if (user?.displayName) {
      setEditName(user.displayName);
    }
  }, [user]);

  // Fetch usage stats
  const { data: usageData, isLoading: isUsageLoading } = useQuery<{ data: UserUsage }>({
    queryKey: ["user-usage"],
    queryFn: () => api.get("/api/v1/user/usage"),
  });
  const usage = usageData?.data;

  // Fetch collections
  const { data: collectionsData } = useQuery<{ data: Array<{ id: string; name: string; color: string }> }>({
    queryKey: ["collections"],
    queryFn: () => api.get("/api/v1/collections"),
  });
  const collectionsList = collectionsData?.data || [];

  // Fetch files for tags/collection computation
  const { data: filesData } = useQuery<{ data: any[] }>({
    queryKey: ["files"],
    queryFn: () => api.get("/api/v1/files"),
  });
  const filesList = filesData?.data || [];

  // Fetch recent activity logs
  const { data: activityData, isLoading: isActivityLoading } = useQuery<{ data: ActivityLog[] }>({
    queryKey: ["activity"],
    queryFn: () => api.get("/api/v1/activity"),
  });
  const recentActivities = (activityData?.data || []).slice(0, 5);

  // Update display name mutation
  const { mutate: updateProfile, isPending: isUpdating } = useMutation({
    mutationFn: async (newName: string) => {
      return api.patch("/api/v1/me", { displayName: newName });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["user"] });
      toast.success("Profile updated successfully.");
      setIsEditingName(false);
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to update profile.");
    },
  });

  const handleRenameSubmit = () => {
    const trimmed = editName.trim();
    if (trimmed && trimmed !== user?.displayName) {
      updateProfile(trimmed);
    } else {
      setIsEditingName(false);
      if (user?.displayName) setEditName(user.displayName);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      handleRenameSubmit();
    } else if (e.key === "Escape") {
      setIsEditingName(false);
      if (user?.displayName) setEditName(user.displayName);
    }
  };

  // Helper to format bytes cleanly
  const formatBytes = (bytes: number) => {
    if (bytes === 0) return "0 Bytes";
    const k = 1024;
    const sizes = ["Bytes", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  };

  // Compute top 10 tags
  const tagCounts: Record<string, number> = {};
  filesList.forEach((file) => {
    file.tags?.forEach((t: string) => {
      tagCounts[t] = (tagCounts[t] || 0) + 1;
    });
  });
  const topTags = Object.entries(tagCounts)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 10);

  // Compute most active collection
  const colCounts: Record<string, number> = {};
  filesList.forEach((file) => {
    if (file.collectionId) {
      colCounts[file.collectionId] = (colCounts[file.collectionId] || 0) + 1;
    }
  });
  const sortedColCounts = Object.entries(colCounts).sort((a, b) => b[1] - a[1]);
  const activeColId = sortedColCounts[0]?.[0];
  const activeColCount = sortedColCounts[0]?.[1] || 0;
  const activeCollection = collectionsList.find((c) => c.id === activeColId);

  // User initials
  const initials = user?.displayName
    ? user.displayName.split(" ").map((n: string) => n[0]).join("").toUpperCase().slice(0, 2)
    : user?.email?.[0]?.toUpperCase() || "U";

  return (
    <ErrorBoundary>
      <div className="space-y-10 select-none font-sans max-w-4xl mx-auto pb-16">
        
        {/* Stark Page Title */}
        <div className="border-b border-border-default pb-6 select-none">
          <span className="text-xs font-semibold uppercase tracking-wider text-accent-primary">
            Profile
          </span>
          <h1 className="font-serif text-4xl font-normal tracking-tight text-text-primary mt-1 select-text selection:bg-accent-primary/20">
            Archive Identity.
          </h1>
        </div>

        {/* 1. Large Identity Header Frame */}
        <div className="bg-bg-surface border border-border-default rounded-2xl p-6 md:p-8 flex flex-col md:flex-row items-center md:items-start gap-6 shadow-sm select-none">
          {/* Initials Circle Avatar */}
          <div className="h-20 w-20 rounded-full bg-accent-subtle border border-accent-subtle-border text-accent-primary flex items-center justify-center font-serif text-3xl font-normal shrink-0 select-none shadow-inner uppercase tracking-wider">
            {initials}
          </div>

          <div className="flex-1 space-y-4 text-center md:text-left min-w-0">
            <div className="space-y-1">
              {isEditingName ? (
                <div className="flex items-center justify-center md:justify-start gap-2">
                  <input
                    type="text"
                    value={editName}
                    onChange={(e) => setEditName(e.target.value)}
                    onKeyDown={handleKeyDown}
                    onBlur={handleRenameSubmit}
                    className="bg-bg-base border border-accent-primary focus:outline-none rounded px-3 py-1.5 text-xl font-semibold text-text-primary max-w-xs w-full"
                    autoFocus
                  />
                  <button
                    onClick={handleRenameSubmit}
                    className="p-2 border border-border-strong bg-bg-surface-raised rounded-lg text-state-success hover:bg-bg-surface-hover shrink-0"
                    title="Confirm rename"
                  >
                    {isUpdating ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <Check className="h-4 w-4" />
                    )}
                  </button>
                </div>
              ) : (
                <h2 
                  onDoubleClick={() => setIsEditingName(true)}
                  className="text-2xl font-serif font-normal text-text-primary hover:text-accent-primary cursor-pointer truncate flex items-center justify-center md:justify-start gap-2 group"
                  title="Double-click to rename"
                >
                  {user?.displayName || "Anonymous Cognizant"}
                  <span className="text-[9px] text-text-dim border border-border-default px-2 py-0.5 rounded font-sans uppercase font-bold tracking-widest opacity-0 group-hover:opacity-100 transition-opacity">
                    Double-click to Rename
                  </span>
                </h2>
              )}
              
              <div className="flex flex-col sm:flex-row items-center justify-center md:justify-start gap-x-4 gap-y-1 text-xs text-text-muted">
                <span className="flex items-center gap-1.5 truncate max-w-xs" title={user?.email}>
                  <Mail className="h-3.5 w-3.5" />
                  {user?.email}
                </span>
                <span className="hidden sm:inline text-text-dim">·</span>
                <span className="flex items-center gap-1.5">
                  <Calendar className="h-3.5 w-3.5" />
                  Member since {usage?.userCreatedAt ? new Date(usage.userCreatedAt).toLocaleDateString(undefined, { dateStyle: "medium" }) : "N/A"}
                </span>
              </div>
            </div>
          </div>
        </div>

        {/* 2. Stat metrics Grid */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 select-none font-sans">
          <div className="p-5 bg-bg-surface border border-border-default rounded-xl shadow-sm text-center">
            <div className="p-2 bg-accent-subtle border border-accent-subtle-border rounded-lg text-accent-primary w-fit mx-auto mb-2">
              <FileText className="h-4 w-4" />
            </div>
            <span className="text-[10px] uppercase font-bold tracking-wider text-text-muted">Files</span>
            <span className="text-2xl font-serif font-normal text-text-primary block mt-1">
              {isUsageLoading ? "..." : usage?.fileCount ?? 0}
            </span>
          </div>

          <div className="p-5 bg-bg-surface border border-border-default rounded-xl shadow-sm text-center">
            <div className="p-2 bg-accent-subtle border border-accent-subtle-border rounded-lg text-accent-primary w-fit mx-auto mb-2">
              <Brain className="h-4 w-4" />
            </div>
            <span className="text-[10px] uppercase font-bold tracking-wider text-text-muted">Passages</span>
            <span className="text-2xl font-serif font-normal text-text-primary block mt-1">
              {isUsageLoading ? "..." : usage?.chunkCount ?? 0}
            </span>
          </div>

          <div className="p-5 bg-bg-surface border border-border-default rounded-xl shadow-sm text-center">
            <div className="p-2 bg-accent-subtle border border-accent-subtle-border rounded-lg text-accent-primary w-fit mx-auto mb-2">
              <HardDrive className="h-4 w-4" />
            </div>
            <span className="text-[10px] uppercase font-bold tracking-wider text-text-muted">Storage Used</span>
            <span className="text-2xl font-serif font-normal text-text-primary block mt-1 truncate">
              {isUsageLoading ? "..." : usage ? formatBytes(usage.storageUsed) : "0 B"}
            </span>
          </div>

          <div className="p-5 bg-bg-surface border border-border-default rounded-xl shadow-sm text-center">
            <div className="p-2 bg-accent-subtle border border-accent-subtle-border rounded-lg text-accent-primary w-fit mx-auto mb-2">
              <Folder className="h-4 w-4" />
            </div>
            <span className="text-[10px] uppercase font-bold tracking-wider text-text-muted">Collections</span>
            <span className="text-2xl font-serif font-normal text-text-primary block mt-1">
              {collectionsList.length}
            </span>
          </div>
        </div>

        {/* 3. Deep Navigation Insights Deck */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6 select-none font-sans">
          
          {/* Tag Cloud Card */}
          <div className="bg-bg-surface border border-border-default rounded-2xl p-6 shadow-sm flex flex-col gap-4">
            <div className="flex items-center gap-2 border-b border-border-default/50 pb-3">
              <Tag className="h-4 w-4 text-accent-primary" />
              <h3 className="text-xs font-semibold text-text-primary uppercase tracking-wider">
                Top Tag Insights
              </h3>
            </div>

            <div className="flex flex-wrap gap-2 items-center justify-center min-h-[120px] p-2 bg-bg-base/30 rounded-xl border border-border-default/50 select-none">
              {topTags.map(([tag, count]) => {
                // Compute font sizes proportionally (between 11px and 16px)
                const maxCount = topTags[0]?.[1] || 1;
                const size = 11 + Math.round((count / maxCount) * 5);
                return (
                  <button
                    key={tag}
                    onClick={() => navigate(`/search?q=${tag}`)}
                    className="px-3 py-1.5 rounded border border-border-default hover:border-border-strong hover:text-text-primary transition-all duration-[120ms] ease-motion cursor-pointer font-bold uppercase tracking-wider bg-bg-surface text-text-muted"
                    style={{ fontSize: `${size}px` }}
                    title={`${count} occurrences`}
                  >
                    {tag}
                  </button>
                );
              })}
              {topTags.length === 0 && (
                <span className="text-xs text-text-dim italic">No tag signatures indexed yet.</span>
              )}
            </div>
          </div>

          {/* Active Collection Workspace Card */}
          <div className="bg-bg-surface border border-border-default rounded-2xl p-6 shadow-sm flex flex-col justify-between gap-4">
            <div className="space-y-4">
              <div className="flex items-center gap-2 border-b border-border-default/50 pb-3">
                <Folder className="h-4 w-4 text-accent-primary" />
                <h3 className="text-xs font-semibold text-text-primary uppercase tracking-wider">
                  Top Workspace Space
                </h3>
              </div>

              {activeCollection ? (
                <div className="p-4 border border-border-default rounded-xl bg-bg-base/30 space-y-2 select-none animate-scale">
                  <div className="flex items-center gap-2">
                    <span className="h-2.5 w-2.5 rounded-full shrink-0" style={{ backgroundColor: activeCollection.color }} />
                    <h4 className="text-sm font-semibold text-text-primary truncate">{activeCollection.name}</h4>
                  </div>
                  <p className="text-xs text-text-muted">
                    This partition is currently your most active workspace, holding **{activeColCount}** files.
                  </p>
                </div>
              ) : (
                <div className="p-4 border border-border-default rounded-xl bg-bg-base/30 text-center select-none text-xs text-text-dim italic">
                  No active collection workspaces found.
                </div>
              )}
            </div>

            {activeCollection && (
              <Link
                to={`/collections/${activeCollection.id}`}
                className="w-full flex items-center justify-between px-4 py-2.5 bg-bg-base border border-border-default hover:border-border-strong hover:bg-bg-surface-raised rounded-xl text-xs font-semibold text-text-primary transition-all cursor-pointer shadow-sm mt-2"
              >
                <span>Explore Workspace detail</span>
                <ChevronRight className="h-4 w-4" />
              </Link>
            )}
          </div>
        </div>

        {/* 4. Recent Audit Logs Timeline */}
        <div className="bg-bg-surface border border-border-default rounded-2xl p-6 shadow-sm space-y-4">
          <div className="flex items-center gap-2 border-b border-border-default pb-3">
            <Activity className="h-4 w-4 text-accent-primary" />
            <h3 className="text-xs font-semibold text-text-primary uppercase tracking-wider">
              Secure Audit Logs
            </h3>
          </div>

          <div className="space-y-3 font-mono text-[11px]">
            {isActivityLoading && (
              <div className="flex justify-center py-6">
                <Loader2 className="h-5 w-5 animate-spin text-text-dim" />
              </div>
            )}
            
            {recentActivities.map((log) => {
              const formattedDate = new Date(log.createdAt).toLocaleDateString(undefined, { dateStyle: "short" }) + " " + new Date(log.createdAt).toLocaleTimeString(undefined, { timeStyle: "short" });
              return (
                <div key={log.id} className="flex justify-between items-center p-2.5 border border-border-default/60 hover:border-border-default rounded-lg bg-bg-base/30 text-text-muted hover:text-text-primary transition-colors">
                  <div className="flex items-center gap-2.5 truncate">
                    <span className="text-[10px] font-bold text-accent-primary border border-accent-subtle-border bg-accent-subtle px-1.5 py-0.5 rounded tracking-wide shrink-0">
                      {log.action}
                    </span>
                    <span className="truncate max-w-[200px] sm:max-w-xs font-semibold" title={log.fileName || undefined}>
                      {log.fileName || "Identity Profile Session"}
                    </span>
                  </div>
                  <div className="flex items-center gap-2 text-text-dim text-[10px] shrink-0">
                    <span>{log.ipAddress}</span>
                    <span>·</span>
                    <span>{formattedDate}</span>
                  </div>
                </div>
              );
            })}

            {recentActivities.length === 0 && !isActivityLoading && (
              <span className="text-xs text-text-dim italic block py-4 text-center">
                No system activity logs recorded.
              </span>
            )}
          </div>

          {recentActivities.length > 0 && (
            <Link
              to="/activity"
              className="w-fit flex items-center gap-1 text-[11px] font-bold uppercase tracking-wider text-accent-primary hover:underline pl-1"
            >
              <span>View full audit timeline</span>
              <ChevronRight className="h-3 w-3" />
            </Link>
          )}
        </div>
      </div>
    </ErrorBoundary>
  );
}
