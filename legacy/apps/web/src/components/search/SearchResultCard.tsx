import { useState, useRef, useEffect } from "react";
import { Calendar, ArrowRight, ArrowDown } from "lucide-react";
import { getFileTypeIcon, formatBytes } from "../files/FileCard";

export interface SearchResult {
  chunkId: string;
  fileId: string;
  fileName: string;
  mimeType: string;
  sizeBytes: number;
  tags: string[];
  summary: string;
  chunkIndex: number;
  createdAt: string;
  rank: number;
  snippet: string;
  matchCount: number;
  scoreComponents?: {
    keywordScore: number;
    semanticScore: number;
    keywordRank: number;
    semanticRank: number;
    rrfScore: number | null;
    rerankScore: number | null;
  };
  allChunks?: Array<{
    chunkId: string;
    chunkIndex: number;
    snippet: string;
    rank: number;
  }>;
}

interface SearchResultCardProps {
  result: SearchResult;
  onOpen: () => void;
  isFocused?: boolean;
}

export function SearchResultCard({ result, onOpen, isFocused = false }: SearchResultCardProps) {
  const [showScoreDetail, setShowScoreDetail] = useState(false);
  const [showPreview, setShowPreview] = useState(false);
  const [isExpanded, setIsExpanded] = useState(false);
  
  const hoverTimer = useRef<any>(null);

  // Format score into match percentage index
  const matchPercentage = Math.round(result.rank * 100);

  const handleMouseEnter = () => {
    if (hoverTimer.current) clearTimeout(hoverTimer.current);
    hoverTimer.current = setTimeout(() => {
      setShowPreview(true);
    }, 600);
  };

  const handleMouseMove = () => {
    if (hoverTimer.current) clearTimeout(hoverTimer.current);
    if (!showPreview) {
      hoverTimer.current = setTimeout(() => {
        setShowPreview(true);
      }, 600);
    }
  };

  const handleMouseLeave = () => {
    if (hoverTimer.current) clearTimeout(hoverTimer.current);
    setShowPreview(false);
  };

  useEffect(() => {
    return () => {
      if (hoverTimer.current) clearTimeout(hoverTimer.current);
    };
  }, []);

  return (
    <div
      id={`chunk-${result.chunkId}`}
      onMouseEnter={handleMouseEnter}
      onMouseMove={handleMouseMove}
      onMouseLeave={handleMouseLeave}
      className={`bg-bg-surface border rounded-xl p-6 transition-all flex flex-col justify-between gap-5 group select-none relative ${
        isFocused 
          ? "border-accent-primary ring-2 ring-accent-primary/20 scale-[1.01] shadow-lg" 
          : "border-border-default hover:border-border-strong"
      }`}
    >
      {/* Floating Preview Popover on stationary hover */}
      {showPreview && (
        <div className="absolute top-2 right-2 md:translate-x-[102%] md:top-0 md:right-0 w-72 md:w-80 z-50 bg-bg-surface border border-border-strong p-4 rounded-2xl shadow-2xl space-y-3 text-left pointer-events-none select-none font-sans animate-fade-in">
          <div className="flex items-center gap-2 border-b border-border-default/50 pb-2">
            <span className="h-1.5 w-1.5 rounded-full bg-accent-primary animate-pulse shrink-0" />
            <span className="text-xs font-medium text-text-muted">
              Preview
            </span>
          </div>
          {result.summary && (
            <div className="space-y-1">
              <span className="text-[10px] font-semibold uppercase tracking-wide text-text-dim block">AI Summary</span>
              <p className="text-[11px] text-text-muted italic leading-relaxed">"{result.summary}"</p>
            </div>
          )}
          <div className="space-y-1">
            <span className="text-[10px] font-semibold uppercase tracking-wide text-text-dim block">Matched passage</span>
            <div 
              className="text-[11px] text-text-primary bg-bg-base border border-border-default rounded-lg p-2.5 whitespace-pre-wrap font-mono leading-relaxed max-h-48 overflow-y-auto"
              dangerouslySetInnerHTML={{ __html: result.snippet }}
            />
          </div>
        </div>
      )}
      {/* Header */}
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-center gap-3.5 min-w-0">
          <div className="p-3 bg-bg-base border border-border-default rounded-xl shrink-0 group-hover:border-border-strong group-hover:bg-bg-surface-raised transition-all">
            {getFileTypeIcon(result.mimeType)}
          </div>
          <div className="min-w-0">
            <h4
              className="font-sans text-sm font-semibold tracking-tight text-text-primary group-hover:text-accent-primary transition-colors truncate selection:bg-accent-primary/20 select-text"
              title={result.fileName}
            >
              {result.fileName}
            </h4>
            <div className="flex items-center gap-2 text-xs text-text-muted mt-1 font-mono">
              <span>{result.mimeType.split("/")[1]}</span>
              <span>·</span>
              <span>{formatBytes(result.sizeBytes)}</span>
              <span>·</span>
              <span className="flex items-center gap-1">
                <Calendar className="h-3 w-3" />
                {new Date(result.createdAt).toLocaleDateString(undefined, { dateStyle: "medium" })}
              </span>
            </div>
          </div>
        </div>

        {/* Unified rank score badge with retrieval explainability metrics */}
        <div className="flex flex-col items-end gap-1.5 shrink-0 select-none">
          <div className="flex items-center gap-1.5">
            {result.scoreComponents && (
              <button
                onClick={() => setShowScoreDetail(!showScoreDetail)}
                className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider border border-border-default hover:border-border-strong text-text-muted hover:text-text-primary bg-bg-base hover:bg-bg-surface-raised transition-all cursor-pointer select-none font-mono"
                title="Toggle score metrics"
              >
                <span>Metrics</span>
                <span className="text-[9px] font-mono leading-none">{showScoreDetail ? "▲" : "▼"}</span>
              </button>
            )}
             <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-bold tracking-wider border bg-accent-subtle border-accent-subtle-border text-accent-primary select-none">
               {matchPercentage >= 40 ? `${matchPercentage}%` : "Low match"}
             </span>
          </div>

          {result.scoreComponents && showScoreDetail && (
            <div className="mt-1 bg-bg-base border border-border-default rounded-xl p-3 text-xs font-mono text-text-muted space-y-1.5 w-48 text-left animate-fade-in shadow-xl select-text">
              <div className="font-bold uppercase border-b border-border-default/50 pb-1 mb-1 text-[10px] tracking-wider text-text-primary select-none">
                Match breakdown
              </div>
              <div className="flex justify-between">
                <span>Keyword Score:</span>
                <span className="font-semibold text-text-primary">{result.scoreComponents.keywordScore.toFixed(3)}</span>
              </div>
              <div className="flex justify-between">
                <span>Keyword Rank:</span>
                <span className="font-semibold text-text-primary">{result.scoreComponents.keywordRank < 9999 ? `#${result.scoreComponents.keywordRank}` : "N/A"}</span>
              </div>
              <div className="flex justify-between">
                <span>Semantic Score:</span>
                <span className="font-semibold text-text-primary">{result.scoreComponents.semanticScore.toFixed(3)}</span>
              </div>
              <div className="flex justify-between">
                <span>Semantic Rank:</span>
                <span className="font-semibold text-text-primary">{result.scoreComponents.semanticRank < 9999 ? `#${result.scoreComponents.semanticRank}` : "N/A"}</span>
              </div>
              {result.scoreComponents.rrfScore !== null && (
                <div className="flex justify-between">
                  <span>RRF Score:</span>
                  <span className="font-semibold text-text-primary">{result.scoreComponents.rrfScore.toFixed(4)}</span>
                </div>
              )}
              {result.scoreComponents.rerankScore !== null && (
                <div className="flex justify-between border-t border-border-default/50 pt-1 mt-1">
                  <span>Rerank Score:</span>
                  <span className="font-semibold text-text-primary">{result.scoreComponents.rerankScore.toFixed(4)}</span>
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Auto-tags row */}
      {result.tags && result.tags.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 pl-1">
          {result.tags.slice(0, 3).map((tag) => (
            <span
              key={tag}
              className="px-2.5 py-0.5 rounded-full text-xs font-semibold bg-bg-surface-raised border border-border-default text-text-muted max-w-[125px] truncate"
            >
              {tag}
            </span>
          ))}
        </div>
      )}

      {/* Document AI summary block */}
      {result.summary && (
        <p className="text-xs leading-relaxed text-text-muted pl-1 select-text selection:bg-accent-primary/20 italic">
          "{result.summary}"
        </p>
      )}

      {/* Highlighted text snippet */}
      <div className="space-y-2 pl-1">
        <span className="text-xs font-semibold uppercase tracking-wide text-text-dim block">
          Matched passage
        </span>
        <div
          className="text-xs text-text-primary bg-bg-base border border-border-default rounded-xl p-4 leading-relaxed whitespace-pre-wrap font-mono select-text border-l-2 border-l-accent-primary selection:bg-accent-primary/20"
          dangerouslySetInnerHTML={{ __html: result.snippet }}
        />
      </div>

      {/* Occurrences count expander */}
      {result.allChunks && result.allChunks.filter((c) => c.chunkId !== result.chunkId).length > 0 && (
        <div className="space-y-3 pl-1">
          <button
            onClick={() => setIsExpanded(!isExpanded)}
            className="flex items-center gap-1.5 py-1 text-xs font-bold uppercase tracking-wider text-accent-primary hover:text-accent-hover transition-colors cursor-pointer"
          >
            <ArrowDown className={`h-3.5 w-3.5 shrink-0 transition-transform duration-[200ms] ${isExpanded ? "rotate-180" : ""}`} />
            <span>
              {isExpanded ? "Hide" : "Show"} {result.allChunks.filter((c) => c.chunkId !== result.chunkId).length} other matched passage{result.allChunks.filter((c) => c.chunkId !== result.chunkId).length === 1 ? "" : "s"}
            </span>
          </button>

          {isExpanded && (
            <div className="space-y-4 pt-2 border-t border-border-default/30 animate-fade-in select-text">
              {result.allChunks.filter((c) => c.chunkId !== result.chunkId).map((chunk) => (
                <div key={chunk.chunkId} className="space-y-2 border-l border-border-default pl-4">
                  <div className="flex justify-between items-center text-[10px] uppercase font-bold tracking-widest text-text-dim">
                    <span>Passage #{chunk.chunkIndex + 1}</span>
                    <span className="font-mono text-accent-primary">Match rank: {(chunk.rank * 100).toFixed(0)}%</span>
                  </div>
                  <div
                    className="text-xs text-text-primary bg-bg-base border border-border-default rounded-xl p-4 leading-relaxed whitespace-pre-wrap font-mono"
                    dangerouslySetInnerHTML={{ __html: chunk.snippet }}
                  />
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Footer view context */}
      <div className="flex justify-end pt-3 border-t border-border-default">
        <button
          onClick={onOpen}
          className="flex items-center gap-1.5 text-xs font-semibold text-accent-primary hover:text-accent-hover transition-colors cursor-pointer group/btn"
        >
          <span className="animated-link-underline pb-0.5">View Full Context</span>
          <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover/btn:translate-x-1 shrink-0" />
        </button>
      </div>
    </div>
  );
}
