/**
 * AKASHA Query Preprocessing Service
 * Normalizes, expands, and segments user queries for higher recall and precision.
 */

// Common acronym expansions for knowledge-domain retrieval
const ACRONYM_MAP: Record<string, string> = {
  "ml": "machine learning",
  "dl": "deep learning",
  "nlp": "natural language processing",
  "cv": "computer vision",
  "ai": "artificial intelligence",
  "llm": "large language model",
  "rag": "retrieval augmented generation",
  "ocr": "optical character recognition",
  "api": "application programming interface",
  "db": "database",
  "sql": "structured query language",
  "css": "cascading style sheets",
  "html": "hypertext markup language",
  "js": "javascript",
  "ts": "typescript",
  "ui": "user interface",
  "ux": "user experience",
  "k8s": "kubernetes",
  "ci": "continuous integration",
  "cd": "continuous deployment",
  "gpu": "graphics processing unit",
  "cpu": "central processing unit",
  "cnn": "convolutional neural network",
  "rnn": "recurrent neural network",
  "gan": "generative adversarial network",
  "bert": "bidirectional encoder representations from transformers",
  "gpt": "generative pre-trained transformer",
  "lstm": "long short term memory",
  "vit": "vision transformer",
  "rlhf": "reinforcement learning from human feedback",
  "dpo": "direct preference optimization",
  "sft": "supervised fine tuning",
};

// Low-information stop words to strip from keyword queries
const STOP_WORDS = new Set([
  "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
  "have", "has", "had", "do", "does", "did", "will", "would", "could",
  "should", "may", "might", "can", "shall", "to", "of", "in", "for",
  "on", "with", "at", "by", "from", "as", "into", "through", "during",
  "before", "after", "above", "below", "between", "out", "off", "over",
  "under", "again", "further", "then", "once", "here", "there", "when",
  "where", "why", "how", "all", "each", "every", "both", "few", "more",
  "most", "other", "some", "such", "no", "nor", "not", "only", "own",
  "same", "so", "than", "too", "very", "just", "about", "up", "it",
  "its", "this", "that", "these", "those", "i", "me", "my", "we", "our",
  "you", "your", "he", "him", "his", "she", "her", "they", "them", "their",
  "what", "which", "who", "whom",
]);

export interface PreprocessedQuery {
  /** Normalized version of the original query */
  normalized: string;
  /** Original query preserved for semantic embedding (casing matters for semantics) */
  forSemantic: string;
  /** Expanded keyword terms for tsquery (includes acronym expansions) */
  forKeyword: string;
  /** Whether the query was expanded */
  wasExpanded: boolean;
  /** Expansion details for telemetry */
  expansions: string[];
  /** Optional auto-detected start date limit */
  detectedFrom?: string;
  /** Suggest standard semantic fallback if keyword yields zero results */
  suggestSemanticFallback?: boolean;
}

/**
 * Automatically detects temporal intent in queries and returns parsed start date ISO string.
 */
function detectDateIntent(query: string): string | null {
  if (/last week/i.test(query)) return new Date(Date.now() - 7 * 86400000).toISOString();
  if (/last month/i.test(query)) return new Date(Date.now() - 30 * 86400000).toISOString();
  if (/yesterday/i.test(query)) return new Date(Date.now() - 86400000).toISOString();
  if (/this week/i.test(query)) return new Date(Date.now() - 7 * 86400000).toISOString();
  return null;
}

/**
 * Preprocess a raw user query for optimal retrieval performance.
 */
export function preprocessQuery(rawQuery: string): PreprocessedQuery {
  const trimmed = rawQuery.trim().replace(/\s+/g, " ");
  const expansions: string[] = [];

  // Preserve original for semantic embedding (casing matters for model understanding)
  const forSemantic = trimmed;

  // Build keyword query: lowercase, strip stop words, expand acronyms
  const tokens = trimmed.toLowerCase().split(/\s+/);

  // Expand acronyms
  const expandedTokens: string[] = [];
  for (const token of tokens) {
    const clean = token.replace(/[^\w]/g, "");
    if (ACRONYM_MAP[clean]) {
      expandedTokens.push(token); // Keep original token
      expandedTokens.push(ACRONYM_MAP[clean]); // Add expansion
      expansions.push(`${clean.toUpperCase()} → ${ACRONYM_MAP[clean]}`);
    } else {
      expandedTokens.push(token);
    }
  }

  // Remove stop words for keyword search (but keep at least 1 token)
  const filteredTokens = expandedTokens.filter((t) => {
    const clean = t.replace(/[^\w]/g, "");
    return !STOP_WORDS.has(clean);
  });

  const forKeyword = (filteredTokens.length > 0 ? filteredTokens : expandedTokens).join(" ").replace(/[<>]/g, "");
  const detectedFrom = detectDateIntent(trimmed) || undefined;

  return {
    normalized: trimmed.toLowerCase(),
    forSemantic,
    forKeyword,
    wasExpanded: expansions.length > 0,
    expansions,
    detectedFrom,
    suggestSemanticFallback: false,
  };
}
