import React from "react";

interface FileGridProps {
  isLoading: boolean;
  children: React.ReactNode;
  skeletonCount?: number;
}

export function FileGrid({ isLoading, children, skeletonCount = 3 }: FileGridProps) {
  if (isLoading) {
    return (
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6">
        {Array.from({ length: skeletonCount }).map((_, i) => (
          <div
            key={i}
            className="bg-bg-surface border border-border-default rounded-card p-6 flex flex-col justify-start animate-pulse select-none"
          >
            <div className="flex items-start gap-4">
              {/* Shimmer icon box */}
              <div className="h-12 w-12 bg-bg-base border border-border-default rounded-xl shrink-0"></div>
              {/* Shimmer text details */}
              <div className="flex-1 space-y-2.5 min-w-0 pr-16">
                <div className="h-4 bg-bg-base border border-border-default/40 rounded w-3/4"></div>
                <div className="h-2.5 bg-bg-base border border-border-default/40 rounded w-1/2"></div>
                <div className="flex items-center gap-1.5 pt-1">
                  <div className="h-4 bg-bg-base border border-border-default/40 rounded w-10"></div>
                  <div className="h-4 bg-bg-base border border-border-default/40 rounded w-14"></div>
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6">
      {children}
    </div>
  );
}
