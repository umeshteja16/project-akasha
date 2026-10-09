import React, { useState, useEffect, useRef } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { 
  MessageSquare, 
  Send, 
  Plus, 
  Trash2, 
  Check, 
  Brain, 
  Loader2, 
  ChevronDown, 
  ChevronUp, 
  History,
  Search
} from "lucide-react";
import { FileItem } from "../../components/files/FileCard";
import { Dialog } from "../../components/ui/Dialog";
import { ErrorBoundary } from "../../components/ui/ErrorBoundary";
import { toast } from "sonner";

interface GroundedCitation {
  id: number;
  chunkId: string;
  fileId: string;
  fileName: string;
  chunkIndex: number;
  snippet: string;
  rank: number;
}

interface Message {
  id: string;
  conversationId: string;
  role: "user" | "assistant";
  content: string;
  citations: GroundedCitation[] | null;
  createdAt: string;
  telemetry?: any;
}

interface Conversation {
  id: string;
  userId: string;
  title: string;
  createdAt: string;
  updatedAt: string;
}

export function ChatPage() {
  const queryClient = useQueryClient();
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // UI state
  const [activeConversationId, setActiveConversationId] = useState<string | null>(null);
  const [inputMessage, setInputMessage] = useState("");
  const [searchMode, setSearchMode] = useState<"keyword" | "semantic" | "hybrid">("hybrid");

  // Scoping context
  const [scopeType, setScopeType] = useState<"all" | "selected" | "collection">("all");
  const [selectedCollectionId, setSelectedCollectionId] = useState<string>("");
  const [selectedFileIds, setSelectedFileIds] = useState<string[]>([]);
  const [filesFilterQuery, setFilesFilterQuery] = useState("");

  // Source visibility toggles (keyed by message ID)
  const [expandedSources, setExpandedSources] = useState<Record<string, boolean>>({});

  // Dialog states
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [convToDelete, setConvToDelete] = useState<{ id: string; title: string } | null>(null);

  // Fetch all user conversations
  const { data: conversationsData, isLoading: isConvsLoading } = useQuery<{ data: Conversation[] }>({
    queryKey: ["conversations"],
    queryFn: () => api.get("/api/v1/conversations"),
  });
  const conversationsList = conversationsData?.data || [];

  // Fetch messages for active conversation
  const { data: messagesData } = useQuery<{ data: Message[] }>({
    queryKey: ["messages", activeConversationId],
    queryFn: () => api.get(`/api/v1/conversations/${activeConversationId}/messages`),
    enabled: !!activeConversationId,
  });
  const messagesList = messagesData?.data || [];

  // Fetch user collections
  const { data: collectionsData } = useQuery<{ data: Array<{ id: string; name: string; color: string }> }>({
    queryKey: ["collections"],
    queryFn: () => api.get("/api/v1/collections"),
  });
  const collectionsList = collectionsData?.data || [];

  // Fetch all user files for selected checklist
  const { data: filesData } = useQuery<{ data: FileItem[] }>({
    queryKey: ["files"],
    queryFn: () => api.get("/api/v1/files"),
  });
  const filesList = filesData?.data || [];
  const filteredChecklistFiles = filesList.filter((f) =>
    f.originalName.toLowerCase().includes(filesFilterQuery.toLowerCase())
  );

  // Auto scroll to bottom
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messagesList]);

  // Handle source toggle
  const toggleSources = (msgId: string) => {
    setExpandedSources((prev) => ({
      ...prev,
      [msgId]: !prev[msgId],
    }));
  };

  // Grounded conversational mutation
  const { mutate: sendMessage, isPending: isSending } = useMutation({
    mutationFn: async (payload: { q: string }) => {
      const body: any = {
        q: payload.q,
        mode: searchMode,
      };
      if (activeConversationId) body.conversationId = activeConversationId;
      if (scopeType === "collection" && selectedCollectionId) body.collectionId = selectedCollectionId;
      if (scopeType === "selected" && selectedFileIds.length > 0) body.fileIds = selectedFileIds;

      return api.post("/api/v1/chat/grounded", body);
    },
    onSuccess: (res: any) => {
      setInputMessage("");
      const newConvId = res?.data?.conversationId;
      if (newConvId && newConvId !== activeConversationId) {
        setActiveConversationId(newConvId);
        queryClient.invalidateQueries({ queryKey: ["conversations"] });
      } else {
        queryClient.invalidateQueries({ queryKey: ["messages", activeConversationId] });
      }
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to synthesize grounded answer.");
    },
  });

  // Delete conversation mutation
  const { mutate: deleteConversation } = useMutation({
    mutationFn: async (id: string) => {
      return api.delete(`/api/v1/conversations/${id}`);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["conversations"] });
      toast.success("Conversation deleted.");
      if (activeConversationId === convToDelete?.id) {
        setActiveConversationId(null);
      }
      setDeleteConfirmOpen(false);
      setConvToDelete(null);
    },
    onError: (err: any) => {
      toast.error(err.message || "Failed to delete conversation.");
    },
  });

  const handleSend = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = inputMessage.trim();
    if (!trimmed || isSending) return;
    sendMessage({ q: trimmed });
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      const trimmed = inputMessage.trim();
      if (trimmed && !isSending) {
        sendMessage({ q: trimmed });
      }
    }
  };

  // Group conversations by temporal sections
  const getGroupedConversations = () => {
    const today: Conversation[] = [];
    const yesterday: Conversation[] = [];
    const thisWeek: Conversation[] = [];
    const older: Conversation[] = [];

    const now = new Date();
    const oneDay = 24 * 60 * 60 * 1000;

    conversationsList.forEach((c) => {
      const date = new Date(c.updatedAt);
      const diffMs = now.getTime() - date.getTime();
      const diffDays = Math.floor(diffMs / oneDay);

      if (diffDays === 0 && date.getDate() === now.getDate()) {
        today.push(c);
      } else if (diffDays <= 1) {
        yesterday.push(c);
      } else if (diffDays <= 7) {
        thisWeek.push(c);
      } else {
        older.push(c);
      }
    });

    return { today, yesterday, thisWeek, older };
  };

  const groupedConversations = getGroupedConversations();

  // Citation parser
  const renderMessageMarkdown = (text: string, citations: GroundedCitation[] | null) => {
    if (!text) return null;
    const blocks = text.split("\n");
    const activeCits = citations || [];

    return (
      <div className="space-y-2 select-text selection:bg-accent-primary/20">
        {blocks.map((block, blockIdx) => {
          let content = block.trim();
          if (!content) return <div key={blockIdx} className="h-1.5"></div>;

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
                const cit = activeCits.find((c) => c.id === citationId);

                if (cit) {
                  elements.push(
                    <button
                      key={matchIndex}
                      type="button"
                      className="inline-flex items-center justify-center px-1.5 py-0.5 mx-0.5 rounded text-[10px] font-bold font-mono tracking-tighter bg-accent-subtle hover:bg-accent-primary/20 text-accent-primary border border-accent-subtle-border hover:border-accent-primary/45 transition-all select-none cursor-pointer transform active:scale-95"
                      title={`Source: ${cit.fileName} (Chunk #${cit.chunkIndex + 1}) · Score: ${Math.round(cit.rank * 100)}%`}
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
              headerLevel === 1 ? "text-base font-serif font-bold text-text-primary mt-2 mb-1" :
              headerLevel === 2 ? "text-sm font-serif font-bold text-text-primary mt-1.5 mb-1" :
              "text-xs font-serif font-semibold text-text-primary mt-1 mb-0.5";
            return <div key={blockIdx} className={headerClasses}>{parsedElements}</div>;
          }

          if (isListItem) {
            return (
              <div key={blockIdx} className="flex items-start gap-2 text-xs text-text-primary pl-3 py-0.5 leading-relaxed">
                <span className="text-accent-primary shrink-0 select-none mt-2 h-1 w-1 rounded-full bg-accent-primary"></span>
                <span>{parsedElements}</span>
              </div>
            );
          }

          if (isBlockquote) {
            const borderClass =
              blockquoteType === "NOTE" ? "border-l-accent-primary bg-accent-subtle/5 border-l-2 pl-3 py-1 my-1.5 rounded-r-lg text-text-muted" :
              blockquoteType === "IMPORTANT" ? "border-l-state-warning bg-state-warning/5 border-l-2 pl-3 py-1 my-1.5 rounded-r-lg text-text-muted font-semibold" :
              "border-l-border-strong bg-bg-base/30 border-l-2 pl-3 py-1 my-1.5 rounded-r-lg text-text-muted italic";
            return (
              <div key={blockIdx} className={borderClass}>
                {blockquoteType && (
                  <span className={`text-[8px] uppercase font-bold tracking-wider block mb-0.5 ${blockquoteType === "NOTE" ? "text-accent-primary" : "text-state-warning"}`}>
                    {blockquoteType}
                  </span>
                )}
                {parsedElements}
              </div>
            );
          }

          return <p key={blockIdx} className="text-xs text-text-primary leading-relaxed">{parsedElements}</p>;
        })}
      </div>
    );
  };

  const handleToggleFileCheck = (id: string) => {
    setSelectedFileIds((prev) =>
      prev.includes(id) ? prev.filter((fid) => fid !== id) : [...prev, id]
    );
  };

  const activeContextIndicatorText = () => {
    if (scopeType === "all") return `Searching across ${filesList.length} files`;
    if (scopeType === "collection") {
      const col = collectionsList.find((c) => c.id === selectedCollectionId);
      return col ? `Limited to: ${col.name}` : "Limited to collection";
    }
    if (scopeType === "selected") {
      return `Limited to: ${selectedFileIds.length} selected files`;
    }
    return "";
  };

  return (
    <ErrorBoundary>
      <div className="flex h-[calc(100vh-8rem)] select-none overflow-hidden border border-border-default bg-bg-surface shadow-md rounded-2xl font-sans relative">
        
        {/* 1. Left Side Control Panel */}
        <aside className="w-76 border-r border-border-default flex flex-col shrink-0 bg-bg-base/20 select-none">
          <div className="p-4 border-b border-border-default space-y-4">
            
            {/* New Thread Button */}
            <button
              onClick={() => {
                setActiveConversationId(null);
                setInputMessage("");
              }}
              className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-accent-primary hover:bg-accent-hover text-bg-base rounded-xl text-xs font-semibold uppercase tracking-wider transition-all shadow-sm cursor-pointer"
            >
              <Plus className="h-4 w-4" />
              New Chat
            </button>

            {/* Context Scoping selector */}
            <div className="space-y-2 select-none">
              <label className="text-[9px] uppercase font-bold tracking-widest text-text-dim block">
                Workspace context
              </label>
              
              <div className="grid grid-cols-3 gap-1 bg-bg-base border border-border-default rounded-lg p-0.5 select-none">
                {(["all", "selected", "collection"] as const).map((t) => (
                  <button
                    key={t}
                    type="button"
                    onClick={() => setScopeType(t)}
                    className={`py-1.5 text-[9px] font-bold rounded uppercase tracking-wider transition-all cursor-pointer ${
                      scopeType === t ? "bg-accent-primary text-bg-base" : "text-text-muted hover:text-text-primary"
                    }`}
                  >
                    {t === "all" ? "Global" : t === "selected" ? "Files" : "Space"}
                  </button>
                ))}
              </div>

              {/* Scope selectors renders */}
              {scopeType === "collection" && (
                <div className="animate-fade-in pt-1">
                  <select
                    value={selectedCollectionId}
                    onChange={(e) => setSelectedCollectionId(e.target.value)}
                    className="w-full bg-bg-surface border border-border-default rounded-lg p-2 text-xs text-text-primary font-medium focus:outline-none focus:border-accent-primary"
                  >
                    <option value="">-- Choose collection --</option>
                    {collectionsList.map((col) => (
                      <option key={col.id} value={col.id}>
                        {col.name}
                      </option>
                    ))}
                  </select>
                </div>
              )}

              {scopeType === "selected" && (
                <div className="animate-fade-in pt-1 space-y-2">
                  <div className="relative">
                    <input
                      type="text"
                      placeholder="Search checklist..."
                      value={filesFilterQuery}
                      onChange={(e) => setFilesFilterQuery(e.target.value)}
                      className="w-full bg-bg-surface border border-border-default rounded-lg pl-8 pr-2 py-1.5 text-[11px] text-text-primary font-sans placeholder:text-text-dim focus:outline-none"
                    />
                    <Search className="absolute left-2.5 top-2.5 h-3.5 w-3.5 text-text-dim" />
                  </div>
                  <div className="max-h-36 overflow-y-auto border border-border-default/60 rounded-lg p-1.5 bg-bg-surface space-y-1.5">
                    {filteredChecklistFiles.map((file) => {
                      const isChecked = selectedFileIds.includes(file.id);
                      return (
                        <div
                          key={file.id}
                          onClick={() => handleToggleFileCheck(file.id)}
                          className="flex items-center gap-2 px-2 py-1 hover:bg-bg-base rounded-md cursor-pointer group"
                        >
                          <div className={`h-3.5 w-3.5 rounded border flex items-center justify-center shrink-0 ${isChecked ? "bg-accent-primary border-accent-primary text-bg-base" : "border-border-default group-hover:border-border-strong"}`}>
                            {isChecked && <Check className="h-2.5 w-2.5 text-bg-base stroke-[3]" />}
                          </div>
                          <span className="text-[10px] font-medium text-text-primary truncate" title={file.originalName}>
                            {file.originalName}
                          </span>
                        </div>
                      );
                    })}
                    {filteredChecklistFiles.length === 0 && (
                      <span className="text-[9px] text-text-dim italic px-2 block">No matching files.</span>
                    )}
                  </div>
                </div>
              )}

              {/* Indicator Badge */}
              <div className="text-[10px] font-mono text-text-dim flex items-center gap-1.5 pl-1 py-1">
                <span className="h-1.5 w-1.5 rounded-full bg-accent-primary animate-pulse shrink-0"></span>
                <span className="truncate">{activeContextIndicatorText()}</span>
              </div>
            </div>
          </div>

          {/* Conversations history list */}
          <div className="flex-1 flex flex-col overflow-hidden">
            <div className="px-4 py-2 border-b border-border-default/30 flex items-center gap-1.5 text-text-dim">
              <History className="h-3.5 w-3.5" />
              <span className="text-[9px] uppercase font-bold tracking-widest">Recent Chats</span>
            </div>

            <div className="flex-1 overflow-y-auto p-2 space-y-4 select-none">
              
              {isConvsLoading && (
                <div className="flex items-center justify-center py-8">
                  <Loader2 className="h-5 w-5 animate-spin text-text-dim" />
                </div>
              )}

              {conversationsList.length === 0 && !isConvsLoading && (
                <span className="text-[10px] text-text-dim italic block px-3 py-4 text-center">
                  No past chats started.
                </span>
              )}

              {/* Render date groups */}
              {Object.entries(groupedConversations).map(([groupKey, list]) => {
                if (list.length === 0) return null;
                const groupTitle =
                  groupKey === "today" ? "Today" :
                  groupKey === "yesterday" ? "Yesterday" :
                  groupKey === "thisWeek" ? "Past 7 days" : "Older";

                return (
                  <div key={groupKey} className="space-y-1 select-none">
                    <span className="text-[8px] font-mono uppercase text-text-dim tracking-wider block px-2 mb-1">
                      {groupTitle}
                    </span>
                    <div className="space-y-0.5">
                      {list.map((c) => {
                        const isActive = activeConversationId === c.id;
                        return (
                          <div
                            key={c.id}
                            className={`group/conv-item relative flex items-center rounded-lg overflow-hidden transition-all duration-[120ms] border ${
                              isActive
                                ? "bg-accent-subtle border-accent-subtle-border text-accent-primary"
                                : "hover:bg-bg-surface-raised border-transparent text-text-muted hover:text-text-primary"
                            }`}
                          >
                            <button
                              onClick={() => {
                                setActiveConversationId(c.id);
                                setInputMessage("");
                              }}
                              className="flex-1 text-left px-3 py-2 text-xs font-semibold truncate flex items-center gap-2 cursor-pointer pr-8"
                              title={c.title}
                            >
                              <MessageSquare className="h-3.5 w-3.5 shrink-0" />
                              <span className="truncate">{c.title}</span>
                            </button>
                            
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                setConvToDelete({ id: c.id, title: c.title });
                                setDeleteConfirmOpen(true);
                              }}
                              className="absolute right-2 opacity-0 group-hover/conv-item:opacity-100 p-1 rounded text-text-dim hover:text-state-error hover:bg-bg-base/40 transition-opacity cursor-pointer"
                              title="Delete chat"
                            >
                              <Trash2 className="h-3 w-3" />
                            </button>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        </aside>

        {/* 2. Main Right conversation panel */}
        <section className="flex-1 flex flex-col justify-between overflow-hidden bg-bg-base/5 select-none">
          
          {/* Active conversation Header bar */}
          <header className="px-6 py-4 border-b border-border-default bg-bg-surface flex items-center justify-between select-none shrink-0 z-10">
            <div className="flex items-center gap-3">
              <div className="h-9 w-9 rounded-xl bg-accent-subtle border border-accent-subtle-border text-accent-primary flex items-center justify-center shrink-0">
                <Brain className="h-4.5 w-4.5" />
              </div>
              <div>
                <h4 className="text-xs font-bold text-text-primary uppercase tracking-wider">
                  {activeConversationId 
                    ? conversationsList.find(c => c.id === activeConversationId)?.title || "Conversational Partner"
                    : "New Conversation turn"
                  }
                </h4>
                <p className="text-[10px] text-text-dim font-mono mt-0.5">
                  RAG grounding active · {searchMode} retrieval
                </p>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <label className="text-[10px] font-bold text-text-muted font-mono uppercase mr-1">Rerank Filter:</label>
              <div className="flex bg-bg-base border border-border-default rounded-lg p-0.5">
                {(["keyword", "semantic", "hybrid"] as const).map((m) => (
                  <button
                    key={m}
                    onClick={() => setSearchMode(m)}
                    className={`px-2 py-0.5 rounded text-[9px] uppercase font-bold tracking-wider transition-all cursor-pointer ${searchMode === m ? "bg-text-primary text-bg-base" : "text-text-muted hover:text-text-primary"}`}
                  >
                    {m}
                  </button>
                ))}
              </div>
            </div>
          </header>

          {/* Messages Flow Area */}
          <div className="flex-1 overflow-y-auto p-6 space-y-6 select-none bg-bg-base/10 relative">
            
            {/* Thread starting guides */}
            {!activeConversationId && messagesList.length === 0 && (
              <div className="max-w-md mx-auto text-center space-y-6 pt-16 animate-fade-in select-none">
                <div className="h-16 w-16 rounded-2xl bg-accent-subtle border border-accent-subtle-border text-accent-primary flex items-center justify-center mx-auto shadow-sm">
                  <Brain className="h-7 w-7" />
                </div>
                <div className="space-y-2">
                  <h3 className="font-serif text-2xl font-normal text-text-primary tracking-tight">
                    Ask your archive anything.
                  </h3>
                  <p className="text-xs text-text-muted leading-relaxed font-sans font-medium">
                    AKASHA will scan your vector passages, retrieve context chunks, and synthesize a pristine, completely grounded answer.
                  </p>
                </div>
                
                {/* Suggestions pills */}
                <div className="grid grid-cols-1 gap-2 pt-2 text-left">
                  {[
                    "What are the main insights inside my transformer research notes?",
                    "Summarize recent deadlines from my screenshot uploads.",
                    "Explain residual connections according to my ML files."
                  ].map((p, idx) => (
                    <button
                      key={idx}
                      onClick={() => setInputMessage(p)}
                      className="w-full p-3 border border-border-default hover:border-border-strong hover:bg-bg-surface-raised rounded-xl text-xs font-semibold text-text-primary transition-all text-left truncate cursor-pointer shadow-sm"
                    >
                      {p}
                    </button>
                  ))}
                </div>
              </div>
            )}

            {/* Render message bubbles */}
            {messagesList.map((msg) => {
              const isAssistant = msg.role === "assistant";
              return (
                <div
                  key={msg.id}
                  className={`flex flex-col gap-1.5 animate-fade-in ${
                    isAssistant ? "items-start pr-12 md:pr-24" : "items-end pl-12 md:pl-24"
                  }`}
                >
                  
                  {/* Sender title */}
                  <span className="text-[9px] uppercase font-bold tracking-widest text-text-dim block px-2 select-none">
                    {isAssistant ? "AKASHA AI" : "You"}
                  </span>

                  {/* Message Bubble */}
                  <div
                    className={`p-4 shadow-sm border rounded-2xl text-xs leading-relaxed max-w-full selection:bg-accent-primary/20 ${
                      isAssistant
                        ? "bg-bg-surface border-border-strong text-text-primary"
                        : "bg-accent-subtle border-accent-subtle-border text-text-primary"
                    }`}
                  >
                    {isAssistant ? (
                      <div className="space-y-4">
                        <div>
                          {renderMessageMarkdown(msg.content, msg.citations)}
                        </div>

                        {/* Collapsible sources row */}
                        {msg.citations && msg.citations.length > 0 && (
                          <div className="border-t border-border-default/40 pt-3.5 mt-2">
                            <button
                              onClick={() => toggleSources(msg.id)}
                              className="flex items-center gap-1.5 text-[10px] font-bold text-text-muted hover:text-text-primary uppercase tracking-wider font-mono cursor-pointer transition-colors"
                            >
                              <span>Grounded Sources ({msg.citations.length})</span>
                              {expandedSources[msg.id] ? (
                                <ChevronUp className="h-3.5 w-3.5" />
                              ) : (
                                <ChevronDown className="h-3.5 w-3.5" />
                              )}
                            </button>

                            {expandedSources[msg.id] && (
                              <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 mt-3 animate-fade-in">
                                {msg.citations.map((c) => (
                                  <div
                                    key={c.id}
                                    className="p-2.5 bg-bg-base border border-border-default rounded-xl space-y-1 select-none"
                                  >
                                    <div className="flex items-center gap-2">
                                      <span className="flex items-center justify-center h-4 w-4 rounded text-[9px] font-bold font-mono bg-accent-subtle border border-accent-subtle-border text-accent-primary select-none">
                                        {c.id}
                                      </span>
                                      <span className="text-[10px] font-semibold text-text-primary truncate uppercase tracking-wider block" title={c.fileName}>
                                        {c.fileName}
                                      </span>
                                    </div>
                                    <p className="text-[9px] leading-relaxed text-text-muted line-clamp-2 select-text selection:bg-accent-primary/20" title={c.snippet}>
                                      "{c.snippet}"
                                    </p>
                                  </div>
                                ))}
                              </div>
                            )}
                          </div>
                        )}
                      </div>
                    ) : (
                      <p className="whitespace-pre-wrap select-text">{msg.content}</p>
                    )}
                  </div>
                </div>
              );
            })}

            {/* Thinking indicator turn */}
            {isSending && (
              <div className="flex flex-col gap-1.5 items-start animate-fade-in pr-12">
                <span className="text-[9px] uppercase font-bold tracking-widest text-text-dim block px-2">
                  AKASHA AI
                </span>
                <div className="p-4 shadow-sm border border-border-strong rounded-2xl bg-bg-surface text-xs leading-relaxed w-full sm:max-w-md">
                  <div className="flex items-center gap-2.5 text-xs text-text-muted font-semibold tracking-wide animate-pulse">
                    <Loader2 className="h-4 w-4 text-accent-primary animate-spin" />
                    <span className="font-mono text-[10px]">
                      Synthesizing grounded multi-file response...
                    </span>
                  </div>
                </div>
              </div>
            )}

            {/* Scroll Anchor */}
            <div ref={messagesEndRef} />
          </div>

          {/* Text Input area */}
          <footer className="p-4 border-t border-border-default bg-bg-surface shrink-0 z-10 select-none">
            <form onSubmit={handleSend} className="max-w-4xl mx-auto flex items-end gap-3 select-none">
              
              <div className="flex-1 relative flex items-center bg-bg-base border border-border-default focus-within:border-text-primary rounded-xl overflow-hidden px-1">
                <textarea
                  value={inputMessage}
                  onChange={(e) => setInputMessage(e.target.value)}
                  onKeyDown={handleKeyDown}
                  placeholder="Ask a question about your files... (⌘+Enter to submit)"
                  className="w-full pl-3 pr-10 py-3 text-xs bg-transparent focus:outline-none placeholder:text-text-dim font-sans resize-none max-h-32 text-text-primary font-medium"
                  rows={Math.min(4, inputMessage.split("\n").length || 1)}
                  style={{ minHeight: "44px" }}
                />
              </div>

              <button
                type="submit"
                disabled={!inputMessage.trim() || isSending}
                className="p-3 bg-accent-primary hover:bg-accent-hover disabled:opacity-40 text-bg-base rounded-xl transition-all shadow-sm cursor-pointer shrink-0 flex items-center justify-center"
                title="Send turning query"
              >
                <Send className="h-4 w-4" />
              </button>
            </form>
          </footer>
        </section>

        {/* Delete confirmation dialog */}
        <Dialog
          isOpen={deleteConfirmOpen}
          title="Delete Chat turn history?"
          description={`"${convToDelete?.title || "This chat"}" session will be permanently erased. All message turns will be dropped.`}
          cancelText="Cancel"
          confirmText="Delete History"
          onCancel={() => {
            setDeleteConfirmOpen(false);
            setConvToDelete(null);
          }}
          onConfirm={() => {
            if (convToDelete) deleteConversation(convToDelete.id);
          }}
          variant="danger"
        />
      </div>
    </ErrorBoundary>
  );
}
