import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { ErrorBoundary } from "../../components/ui/ErrorBoundary";
import { CanvasBackground } from "../../components/ui/CanvasBackground";
import { getRelativeTimeString } from "../../components/files/FileCard";
import { 
  History, 
  ArrowLeft, 
  FileUp, 
  Trash2, 
  RotateCw, 
  Activity, 
  Globe, 
  Terminal 
} from "lucide-react";
import { useNavigate } from "react-router";

interface ActivityItem {
  id: string;
  action: string;
  ipAddress: string | null;
  createdAt: string;
  fileId: string | null;
  fileName: string | null;
}

export function ActivityTimelinePage() {
  const navigate = useNavigate();
  const [filter, setFilter] = useState<"ALL" | "FILE_UPLOAD" | "FILE_DELETE" | "SYSTEM">("ALL");

  const { data: activityData, isLoading, error } = useQuery<{ data: ActivityItem[] }>({
    queryKey: ["user-activity"],
    queryFn: () => api.get("/api/v1/activity"),
  });

  const activities = activityData?.data || [];

  const filteredActivities = activities.filter((act) => {
    if (filter === "ALL") return true;
    if (filter === "SYSTEM") {
      return act.action !== "FILE_UPLOAD" && act.action !== "FILE_DELETE";
    }
    return act.action === filter;
  });

  const getActionDetails = (action: string) => {
    switch (action) {
      case "FILE_UPLOAD":
        return {
          label: "File Uploaded",
          color: "text-state-processing bg-state-processing/10 border-state-processing/20",
          icon: <FileUp className="h-4 w-4 shrink-0 text-state-processing" />,
          desc: "Added to your archive and queued for indexing."
        };
      case "FILE_DELETE":
        return {
          label: "File Deleted",
          color: "text-state-error bg-state-error/10 border-state-error/20",
          icon: <Trash2 className="h-4 w-4 shrink-0 text-state-error" />,
          desc: "Removed from your archive."
        };
      case "FILE_REINDEX":
        return {
          label: "Re-indexed",
          color: "text-state-warning bg-state-warning/10 border-state-warning/20",
          icon: <RotateCw className="h-4 w-4 shrink-0 text-state-warning animate-spin" />,
          desc: "Queued for re-processing."
        };
      default:
        return {
          label: action.replace(/_/g, " "),
          color: "text-text-muted bg-border-default/30 border-border-default",
          icon: <Activity className="h-4 w-4 shrink-0 text-text-muted" />,
          desc: "User activity recorded."
        };
    }
  };

  return (
    <ErrorBoundary>
      <div className="space-y-12 select-none relative animate-fade-in font-sans">
        {/* Floating Background stars */}
        {activities.length === 0 && !isLoading && (
          <CanvasBackground intensity="barely" />
        )}

        {/* Back and Page Header */}
        <div className="border-b border-border-default pb-6 flex items-center justify-between">
          <div>
            <button
              onClick={() => navigate(-1)}
              className="flex items-center gap-1.5 text-xs font-semibold text-text-muted hover:text-text-primary transition-colors cursor-pointer group select-none"
            >
              <ArrowLeft className="h-3.5 w-3.5 group-hover:-translate-x-0.5 transition-transform" />
              Go Back
            </button>
            <h2 className="font-serif text-4xl font-normal tracking-tight text-text-primary mt-3 select-text">
              Activity History.
            </h2>
            <p className="text-xs text-text-muted leading-normal mt-1 max-w-xl">
              A record of everything that’s happened in your archive.
            </p>
          </div>
          <div className="p-3 bg-bg-surface border border-border-default rounded-xl shrink-0 select-none shadow-sm">
            <History className="h-6 w-6 text-text-muted" />
          </div>
        </div>

        {/* Loading feed state */}
        {isLoading && (
          <div className="space-y-4 max-w-2xl mx-auto py-12">
            {[1, 2, 3].map((n) => (
              <div key={n} className="border border-border-default rounded-xl p-6 bg-bg-surface/50 animate-pulse flex items-start gap-4">
                <div className="h-8 w-8 rounded bg-border-default shrink-0" />
                <div className="space-y-2 flex-1">
                  <div className="h-3.5 bg-border-default rounded w-1/3" />
                  <div className="h-3 bg-border-default rounded w-2/3" />
                </div>
              </div>
            ))}
          </div>
        )}

        {/* Errors view */}
        {error && (
          <div className="max-w-2xl mx-auto bg-state-error/10 border border-state-error/25 text-state-error text-xs rounded-xl p-5 flex items-start gap-3 select-text">
            <Terminal className="h-5 w-5 mt-0.5 shrink-0" />
            <div>
              <p className="font-bold uppercase tracking-wider text-[10px]">Failed to load activity logs</p>
              <p className="leading-relaxed mt-1">Could not load activity. Check your connection and try again.</p>
            </div>
          </div>
        )}

        {/* Sleek Event Filter Pills */}
        {!isLoading && !error && activities.length > 0 && (
          <div className="flex flex-wrap items-center justify-center gap-2.5 max-w-2xl mx-auto select-none border-b border-border-default pb-6">
            {(["ALL", "FILE_UPLOAD", "FILE_DELETE", "SYSTEM"] as const).map((mode) => {
              const isActive = filter === mode;
              const label = mode === "ALL" ? "All Activity" : mode === "FILE_UPLOAD" ? "Uploads" : mode === "FILE_DELETE" ? "Deletions" : "System / Access";
              return (
                <button
                  key={mode}
                  onClick={() => setFilter(mode)}
                  className={`px-4.5 py-1.5 rounded-full border text-[10px] font-semibold uppercase tracking-wider transition-all duration-[120ms] ease-motion cursor-pointer select-none ${
                    isActive
                      ? "bg-accent-primary border-accent-primary text-bg-base shadow-sm scale-[1.03]"
                      : "bg-bg-surface/50 border-border-default text-text-muted hover:border-border-strong hover:text-text-primary hover:bg-bg-surface-raised"
                  }`}
                >
                  {label}
                </button>
              );
            })}
          </div>
        )}

        {/* Chronological Timeline feed */}
        {!isLoading && !error && filteredActivities.length > 0 && (
          <div className="max-w-3xl mx-auto relative select-none">
            {/* Timeline center line marker */}
            <div className="absolute left-6 top-4 bottom-4 w-px bg-border-default" />

            <div className="space-y-8 select-none">
              {filteredActivities.map((act) => {
                const details = getActionDetails(act.action);
                return (
                  <div key={act.id} className="relative pl-12 flex gap-4 select-none animate-slide-in">
                    {/* Circle marker icon */}
                    <div className="absolute left-[13px] top-1.5 h-6 w-6 rounded-full border border-border-default bg-bg-surface flex items-center justify-center shadow-sm select-none z-10">
                      <span className="h-2 w-2 rounded-full bg-accent-primary" />
                    </div>

                    <div className="w-full bg-bg-surface border border-border-default hover:border-border-strong rounded-xl p-5 transition-all duration-[120ms] ease-motion shadow-sm flex flex-col md:flex-row md:items-center justify-between gap-4 select-none">
                      <div className="space-y-2.5 min-w-0">
                        {/* Status Label and Badge */}
                        <div className="flex items-center gap-2">
                          <span className={`inline-flex items-center gap-1.5 px-2.5 py-0.5 border rounded-full text-[10px] font-semibold uppercase tracking-wider select-none ${details.color}`}>
                            {details.icon}
                            {details.label}
                          </span>
                          <span className="text-[10px] font-mono text-text-dim select-none uppercase tracking-wider">
                            {getRelativeTimeString(act.createdAt)}
                          </span>
                        </div>

                        {/* Description details */}
                        <div className="min-w-0">
                          {act.fileName ? (
                            <p className="text-xs font-semibold text-text-primary select-text leading-relaxed">
                              File: <span className="font-mono text-accent-primary hover:underline cursor-pointer select-text">{act.fileName}</span>
                            </p>
                          ) : (
                            <p className="text-xs text-text-muted select-text leading-relaxed">
                              {details.desc}
                            </p>
                          )}
                        </div>
                      </div>

                      {/* Diagnostic details (IP Address) */}
                      {act.ipAddress && (
                        <div className="flex items-center gap-1.5 text-[10px] text-text-dim font-mono select-none uppercase tracking-widest shrink-0 self-end md:self-center">
                          <Globe className="h-3.5 w-3.5 text-text-dim shrink-0" />
                          <span>{act.ipAddress}</span>
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Empty activities feed fallback */}
        {!isLoading && !error && filteredActivities.length === 0 && (
          <div className="max-w-md mx-auto border border-dashed border-border-default bg-bg-surface/30 rounded-xl p-12 text-center flex flex-col items-center justify-center space-y-4 animate-fade-in select-none">
            <div className="h-10 w-10 rounded-full border border-border-default bg-bg-surface flex items-center justify-center select-none shadow-sm shrink-0">
              <History className="h-5 w-5 text-text-dim" />
            </div>
            <div className="space-y-1.5 select-none">
              <h4 className="text-sm font-semibold text-text-primary">No activity logged</h4>
              <p className="text-xs text-text-muted leading-relaxed font-sans font-medium">
                Your file uploads, deletions, and other actions will appear here.
              </p>
            </div>
          </div>
        )}
      </div>
    </ErrorBoundary>
  );
}
