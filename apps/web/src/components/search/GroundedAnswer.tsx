import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import {
  Brain,
  Check,
  Copy,
  Loader2,
  X,
  RotateCcw,
} from "lucide-react";
import { useState, useEffect } from "react";

export interface GroundedCitation {
  id: number;
  chunkId: string;
  fileId: string;
  fileName: string;
  chunkIndex: number;
  snippet: string;
  rank: number;
}

interface GroundedResponse {
  data: {
    conversationId: string;
    answer: string;
    citations: GroundedCitation[];
    model: string;
    telemetry?: {
      embedding_ms: number;
      db_search_ms: number;
      rerank_ms: number;
      generation_ms: number;
      total_ms: number;
    };
  };
}

interface GroundedAnswerProps {
  query: string;
  searchMode: "keyword" | "semantic" | "hybrid";
  onCitationClick: (citation: GroundedCitation) => void;
  hasRequestedAI: boolean;
  onRequestAI: (val: boolean) => void;
  collectionId?: string;
}

export function GroundedAnswer({
  query,
  searchMode,
  onCitationClick,
  hasRequestedAI,
  onRequestAI,
  collectionId,
}: GroundedAnswerProps) {
  const [copied, setCopied] = useState(false);
  const [activeConversationId, setActiveConversationId] = useState<string | undefined>(undefined);

  // Reset thread when the search query changes
  useEffect(() => {
    setActiveConversationId(undefined);
  }, [query]);

  // RAG Grounded Answer fetch query
  const { data: groundedData, isFetching: isGroundedFetching, error: groundedError } = useQuery<GroundedResponse>({
    queryKey: ["grounded-chat", query, searchMode, activeConversationId, collectionId],
    queryFn: () => {
      return api.post("/api/v1/chat/grounded", {
        q: query,
        mode: searchMode,
        conversationId: activeConversationId,
        collectionId,
      });
    },
    enabled: hasRequestedAI && query.trim().length >= 1,
    retry: 1,
  });

  // Track conversation state updates
  useEffect(() => {
    if (groundedData?.data?.conversationId) {
      setActiveConversationId(groundedData.data.conversationId);
    }
  }, [groundedData]);



  const copyToClipboard = (text: string) => {
    if (copied) return;
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  const parseMarkdownWithCitations = (text: string, citations: GroundedCitation[]) => {
    if (!text) return null;
    const blocks = text.split("\n");

    return (
      <div className="space-y-3.5 select-text selection:bg-accent-primary/20">
        {blocks.map((block, blockIdx) => {
          let content = block.trim();
          if (!content) return <div key={blockIdx} className="h-2"></div>;

          let isBlockquote = false;
          let blockquoteType = "";
          if (content.startsWith(">")) {
            isBlockquote = true;
            content = content.substring(1).trim();
            if (content.startsWith("[!NOTE]")) {
              blockquoteType = "NOTE";
              content = content.substring(7).trim();
            } else if (content.startsWith("[!IMPORTANT]")) {
              blockquoteType = "IMPORTANT";
              content = content.substring(12).trim();
            }
          }

          let isHeader = false;
          let headerLevel = 0;
          if (content.startsWith("#")) {
            const match = content.match(/^(#{1,6})\s+(.*)$/);
            if (match) {
              isHeader = true;
              headerLevel = match[1].length;
              content = match[2];
            }
          }

          let isListItem = false;
          if (content.startsWith("* ") || content.startsWith("- ")) {
            isListItem = true;
            content = content.substring(2);
          }

          const renderInline = (inlineText: string) => {
            const elements = [];
            const regex = /(\*\*.*?\*\*|\[\d+\])/g;
            let lastIndex = 0;
            let match;

            while ((match = regex.exec(inlineText)) !== null) {
              const matchText = match[0];
              const matchIndex = match.index;

              if (matchIndex > lastIndex) {
                elements.push(inlineText.substring(lastIndex, matchIndex));
              }

              if (matchText.startsWith("**") && matchText.endsWith("**")) {
                elements.push(
                  <strong key={matchIndex} className="font-bold text-text-primary">
                    {matchText.slice(2, -2)}
                  </strong>
                );
              } else if (matchText.startsWith("[") && matchText.endsWith("]")) {
                const citationId = parseInt(matchText.slice(1, -1), 10);
                const citation = citations.find((c) => c.id === citationId);

                if (citation) {
                  elements.push(
                    <button
                      key={matchIndex}
                      onClick={() => onCitationClick(citation)}
                      className="inline-flex items-center justify-center px-1.5 py-0.5 mx-0.5 rounded text-[10px] font-bold font-mono tracking-tighter bg-accent-subtle hover:bg-accent-primary/20 text-accent-primary border border-accent-subtle-border hover:border-accent-primary/40 transition-all select-none cursor-pointer transform hover:scale-105 active:scale-95"
                      title={`Source: ${citation.fileName} (Chunk #${citation.chunkIndex + 1}) · Score: ${Math.round(citation.rank * 100)}%`}
                    >
                      {citationId}
                    </button>
                  );
                } else {
                  elements.push(matchText);
                }
              }

              lastIndex = regex.lastIndex;
            }

            if (lastIndex < inlineText.length) {
              elements.push(inlineText.substring(lastIndex));
            }

            return elements;
          };

          const parsedElements = renderInline(content);

          if (isHeader) {
            const headerClasses =
              headerLevel === 1 ? "text-xl font-serif font-bold text-text-primary mt-4 mb-2" :
              headerLevel === 2 ? "text-lg font-serif font-bold text-text-primary mt-3 mb-2" :
              "text-xs font-serif font-semibold text-text-primary mt-2.5 mb-1";
            return <div key={blockIdx} className={headerClasses}>{parsedElements}</div>;
          }

          if (isListItem) {
            return (
              <div key={blockIdx} className="flex items-start gap-2 text-xs text-text-primary pl-4 py-0.5 leading-relaxed">
                <span className="text-accent-primary shrink-0 select-none mt-2 h-1.5 w-1.5 rounded-full bg-accent-primary"></span>
                <span>{parsedElements}</span>
              </div>
            );
          }

          if (isBlockquote) {
            const borderClass =
              blockquoteType === "NOTE" ? "border-l-accent-primary bg-accent-subtle/5 border-l-2 pl-4 py-2 my-2 rounded-r-lg text-text-muted" :
              blockquoteType === "IMPORTANT" ? "border-l-state-warning bg-state-warning/5 border-l-2 pl-4 py-2 my-2 rounded-r-lg text-text-muted font-semibold" :
              "border-l-border-strong bg-bg-base/30 border-l-2 pl-4 py-2 my-2 rounded-r-lg text-text-muted italic";
            return (
              <div key={blockIdx} className={borderClass}>
                {blockquoteType && (
                  <span className={`text-[9px] uppercase font-bold tracking-wider block mb-0.5 ${blockquoteType === "NOTE" ? "text-accent-primary" : "text-state-warning"}`}>
                    {blockquoteType}
                  </span>
                )}
                {parsedElements}
              </div>
            );
          }

          return <p key={blockIdx} className="text-xs text-text-primary leading-loose">{parsedElements}</p>;
        })}
      </div>
    );
  };

  if (!hasRequestedAI) {
    return null;
  }

  const citations = groundedData?.data?.citations || [];
  const topScore = citations.length > 0 ? citations[0].rank : 0;
  
  let confidenceBadge = null;
  if (!isGroundedFetching && citations.length > 0) {
    confidenceBadge = (
      <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[8px] font-bold font-mono tracking-wide bg-accent-subtle border border-accent-subtle-border text-accent-primary select-none">
        {Math.round(topScore * 100)}%
      </span>
    );
  }

  return (
    <div className="relative group/rag select-none">
      <div className="relative bg-bg-surface border border-border-strong rounded-2xl p-6 shadow-xl transition-all duration-300">
        <div className="flex items-center justify-between border-b border-border-default pb-4 mb-4">
          <div className="flex items-center gap-2.5 min-w-0">
            <div className="p-2 bg-accent-subtle border border-accent-subtle-border rounded-xl text-accent-primary shrink-0">
              <Brain className="h-4.5 w-4.5" />
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <h4 className="font-serif text-sm font-semibold text-text-primary">
                  AI Answer
                </h4>
                {confidenceBadge}
              </div>
              <span className="text-[9px] text-text-dim font-mono uppercase bg-bg-base px-2 py-0.5 rounded border border-border-default truncate block max-w-[150px] mt-0.5">
                {groundedData?.data?.model || (isGroundedFetching ? "Synthesizing..." : "Preparing...")}
              </span>
            </div>
          </div>
          
          <div className="flex items-center gap-2 shrink-0">
            {activeConversationId && (
              <button
                onClick={() => {
                  setActiveConversationId(undefined);
                  onRequestAI(false);
                }}
                title="Start new conversation"
                className="flex items-center gap-1 px-2.5 py-1 text-[9px] font-bold uppercase tracking-wider border border-border-default hover:border-border-strong rounded bg-bg-base hover:bg-bg-surface-raised text-text-muted hover:text-text-primary transition-all cursor-pointer mr-1"
              >
                <RotateCcw className="h-3 w-3" />
                New Thread
              </button>
            )}

            <button
              onClick={() => copyToClipboard(groundedData?.data?.answer || "")}
              disabled={!groundedData?.data?.answer}
              title="Copy answer"
              className="p-1.5 text-text-muted hover:text-text-primary hover:bg-bg-base border border-transparent hover:border-border-default rounded-lg transition-all cursor-pointer relative group/copy"
            >
              {copied ? (
                <Check className="h-3.5 w-3.5 text-state-success" />
              ) : (
                <Copy className="h-3.5 w-3.5" />
              )}
              <span className="absolute bottom-full right-1/2 translate-x-1/2 mb-2 px-2 py-1 bg-bg-base border border-border-strong text-[9px] font-bold uppercase rounded opacity-0 group-hover/copy:opacity-100 transition-opacity pointer-events-none whitespace-nowrap">
                {copied ? "Copied!" : "Copy Answer"}
              </span>
            </button>

            <div className="flex items-center gap-1.5 text-[9px] font-bold text-text-muted font-mono uppercase">
              <span className={`h-1.5 w-1.5 rounded-full ${isGroundedFetching ? "bg-accent-primary animate-pulse" : "bg-state-success animate-ping"}`}></span>
              <span>{isGroundedFetching ? "Thinking" : "Sourced"}</span>
            </div>
          </div>
        </div>

        {isGroundedFetching ? (
          <div className="space-y-3.5 py-2">
            <div className="flex items-center gap-2 text-xs text-text-muted font-semibold tracking-wide animate-pulse">
              <Loader2 className="h-3.5 w-3.5 text-accent-primary animate-spin" />
              <span className="font-mono text-[10px]">
                Searching your files and generating an answer...
              </span>
            </div>
            <div className="space-y-2">
              <div className="h-3 bg-bg-base border border-border-default rounded-md w-full animate-pulse"></div>
              <div className="h-3 bg-bg-base border border-border-default rounded-md w-[92%] animate-pulse"></div>
              <div className="h-3 bg-bg-base border border-border-default rounded-md w-[85%] animate-pulse"></div>
            </div>
          </div>
        ) : groundedError ? (
          <div className="flex items-start gap-3 p-4 bg-red-950/20 border border-red-500/20 rounded-xl text-xs text-red-400 animate-fade-in">
            <X className="h-5 w-5 shrink-0 text-red-400" />
            <div>
              <h5 className="font-bold uppercase tracking-wider mb-1">Search failed</h5>
              <p className="text-text-muted leading-relaxed">
                {(groundedError as any).message || "Something went wrong while generating the answer. Please try again."}
              </p>
            </div>
          </div>
        ) : (
          <div className="space-y-4">
            <div className="text-[11px] font-bold uppercase tracking-wider text-text-muted font-mono block">
              AI Answer
            </div>
            <div className="text-xs leading-relaxed text-text-primary">
              {parseMarkdownWithCitations(
                groundedData?.data?.answer || "No response generated.",
                groundedData?.data?.citations || []
              )}
            </div>

            {/* Citations panel layout */}
            {groundedData?.data?.citations && groundedData.data.citations.length > 0 && (
              <div className="border-t border-border-default pt-4 mt-2">
                <span className="text-[10px] uppercase font-bold tracking-wider text-text-muted block mb-2.5">
                  Sources
                </span>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
                  {groundedData.data.citations.map((cit) => (
                    <div
                      key={`cit-card-${cit.id}`}
                      onClick={() => onCitationClick(cit)}
                      className="group/cit-item flex items-center p-3 bg-bg-base hover:bg-bg-surface-raised border border-border-default hover:border-border-strong rounded-xl cursor-pointer transition-all duration-300"
                    >
                      <div className="flex items-center gap-2.5 min-w-0">
                        <span className="flex items-center justify-center h-5 w-5 rounded text-[10px] font-bold font-mono bg-accent-subtle border border-accent-subtle-border text-accent-primary shrink-0 select-none group-hover/cit-item:bg-accent-primary group-hover/cit-item:text-bg-base transition-colors">
                          {cit.id}
                        </span>
                        <div className="min-w-0">
                          <h5 className="text-[11px] font-semibold text-text-primary truncate uppercase tracking-wider" title={cit.fileName}>
                            {cit.fileName}
                          </h5>
                          <p className="text-[9px] text-text-dim font-mono mt-0.5">
                            Chunk #{cit.chunkIndex + 1} · Score: {Math.round(cit.rank * 100)}%
                          </p>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Telemetry panel */}
            {groundedData?.data?.telemetry && (
              <div className="border-t border-border-default pt-4 mt-4 flex flex-wrap items-center justify-between gap-3 select-none">
                <span className="text-[9px] font-bold uppercase tracking-wider text-text-muted">
                  Performance
                </span>
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[8.5px] font-mono text-text-dim">
                  <span>Embed: {groundedData.data.telemetry.embedding_ms}ms</span>
                  <span>·</span>
                  <span>DB search: {groundedData.data.telemetry.db_search_ms}ms</span>
                  <span>·</span>
                  <span>Rerank: {groundedData.data.telemetry.rerank_ms}ms</span>
                  <span>·</span>
                  <span>Synthesis: {groundedData.data.telemetry.generation_ms}ms</span>
                  <span>·</span>
                  <span className="font-bold text-accent-primary">Total: {groundedData.data.telemetry.total_ms}ms</span>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
