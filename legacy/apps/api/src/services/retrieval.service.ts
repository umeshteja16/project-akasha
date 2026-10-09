import { files, fileChunks, eq, and, sql } from "@akasha/db";
import { db } from "../db.js";
import { preprocessQuery } from "./query.service.js";
import { validateEnv } from "@akasha/shared";
const env = validateEnv(process.env);

// RRF constant — standard value used by Elasticsearch, Pinecone, etc.
const RRF_K = 60;

export interface FusedResult {
  chunkId: string;
  fileId: string;
  chunkIndex: number;
  chunkText: string;
  fileName: string;
  mimeType: string;
  sizeBytes: number;
  tags: string[] | null;
  summary: string | null;
  createdAt: string | Date;
  rank: number;
  keywordRank?: number;
  semanticRank?: number;
  keywordScore?: number;
  semanticScore?: number;
  rrfScore?: number;
  snippet: string;
  matchCount?: number;
  maxRank?: number;
}

export interface RetrievalOptions {
  userId: string;
  query: string;
  mode: "keyword" | "semantic" | "hybrid";
  type?: "pdf" | "image" | "text" | "media" | "all";
  from?: string;
  generateSnippets?: boolean;
  collectionId?: string;
  fileIds?: string[];
  log?: {
    info(obj: any, msg?: string): void;
    warn(obj: any, msg?: string): void;
    error(obj: any, msg?: string): void;
  };
}

export interface RetrievalResult {
  results: FusedResult[];
  queryEmbedding: number[];
  mode: "keyword" | "semantic" | "hybrid";
  timings: {
    embedding: number;
    dbSearch: number;
    rerank: number;
    total: number;
  };
  suggestSemanticFallback?: boolean;
  spellSuggestion?: string;
}

interface ChunkRow {
  chunkId: string;
  fileId: string;
  chunkIndex: number;
  chunkText: string;
  fileName: string;
  mimeType: string;
  sizeBytes: number;
  tags: string[] | null;
  summary: string | null;
  createdAt: string | Date;
  score: number;
}

export async function retrieveChunks(options: RetrievalOptions): Promise<RetrievalResult> {
  const totalStart = performance.now();
  let embeddingStart = 0, embeddingEnd = 0;
  let dbSearchStart = 0, dbSearchEnd = 0;
  let rerankStart = 0, rerankEnd = 0;
  let suggestSemanticFallback = false;
  let spellSuggestion: string | undefined;

  const { userId, query, type = "all", from, generateSnippets = false, collectionId, fileIds, log } = options;
  let mode = options.mode;

  // Preprocess query
  const processed = preprocessQuery(query);

  let queryEmbedding: number[] = [];

  // Fetch query vector embedding for semantic or hybrid search modes
  if (mode === "semantic" || mode === "hybrid") {
    embeddingStart = performance.now();
    try {
      let embedResponse;
      try {
        embedResponse = await fetch("http://embedding:8000/embed", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ texts: [processed.forSemantic] }),
        });
      } catch (firstErr) {
        embedResponse = await fetch("http://localhost:8000/embed", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ texts: [processed.forSemantic] }),
        });
      }

      if (!embedResponse.ok) {
        const errBody = await embedResponse.text();
        throw new Error(`Embedding service returned ${embedResponse.status}: ${errBody}`);
      }

      const resData = (await embedResponse.json()) as { embeddings: number[][] };
      if (resData.embeddings && resData.embeddings.length > 0) {
        queryEmbedding = resData.embeddings[0];
      }
    } catch (err: any) {
      if (log) {
        log.warn(err, `Failed to generate query embedding. Falling back to keyword-only search.`);
      } else {
        console.warn(`[RetrievalService] Failed to generate query embedding. Falling back to keyword-only:`, err);
      }
      mode = "keyword";
    }
    embeddingEnd = performance.now();
  }

  // Build shared base conditions
  const baseConditions = [eq(files.ownerId, userId)];

  if (collectionId) {
    baseConditions.push(eq(files.collectionId, collectionId));
  }

  if (fileIds && fileIds.length > 0) {
    baseConditions.push(sql`${files.id} IN (${sql.join(fileIds.map(id => sql`${id}`), sql`, `)})`);
  }

  if (type === "pdf") {
    baseConditions.push(eq(files.mimeType, "application/pdf"));
  } else if (type === "image") {
    baseConditions.push(
      sql`${files.mimeType} IN ('image/png', 'image/jpeg', 'image/jpg', 'image/webp')`
    );
  } else if (type === "text") {
    baseConditions.push(
      sql`${files.mimeType} IN ('text/plain', 'text/markdown', 'text/x-markdown', 'text/csv')`
    );
  } else if (type === "media") {
    baseConditions.push(
      sql`(${files.mimeType} LIKE 'video/%' OR ${files.mimeType} LIKE 'audio/%')`
    );
  }

  if (from) {
    const fromDate = new Date(from);
    if (!isNaN(fromDate.getTime())) {
      baseConditions.push(sql`${files.createdAt} >= ${fromDate}`);
    }
  }

  // Use the preprocessed keyword string for tsquery
  const searchQuery = sql`websearch_to_tsquery('english', ${processed.forKeyword})`;

  if (mode === "semantic" || mode === "hybrid") {
    try {
      await db.execute(sql`SET hnsw.ef_search = 100;`);
    } catch (e) {
      // ignore if set session config fails
    }
  }

  dbSearchStart = performance.now();
  const queryTerm = query.trim();

  let fusedResults: FusedResult[] = [];

  if (mode === "keyword") {
    // Pure keyword path
    const keywordConditions = [...baseConditions];
    if (queryTerm.length > 1) {
      keywordConditions.push(sql`(${fileChunks.tsvectorContent} @@ ${searchQuery} OR ${files.originalName} ILIKE ${'%' + queryTerm + '%'})`);
    } else {
      keywordConditions.push(sql`${fileChunks.tsvectorContent} @@ ${searchQuery}`);
    }
    const keywordScoreExpr = sql`ts_rank(${fileChunks.tsvectorContent}, ${searchQuery}, 32)`;

    const keywordHeadlineExpr = generateSnippets
      ? sql`, ts_headline('english', ${fileChunks.chunkText}, ${searchQuery}, 'StartSel=<mark>, StopSel=</mark>, MaxWords=150, MinWords=50') AS "snippet"`
      : sql``;

    const results = await db.execute(sql`
      SELECT
        ${fileChunks.id} AS "chunkId",
        ${fileChunks.fileId} AS "fileId",
        ${fileChunks.chunkIndex} AS "chunkIndex",
        ${fileChunks.chunkText} AS "chunkText",
        ${files.originalName} AS "fileName",
        ${files.mimeType} AS "mimeType",
        ${files.sizeBytes} AS "sizeBytes",
        ${files.tags} AS "tags",
        ${files.summary} AS "summary",
        ${files.createdAt} AS "createdAt",
        (${keywordScoreExpr}) AS "score"
        ${keywordHeadlineExpr}
      FROM ${fileChunks}
      INNER JOIN ${files} ON ${fileChunks.fileId} = ${files.id}
      WHERE ${and(...keywordConditions)}
      ORDER BY (${keywordScoreExpr}) DESC
      LIMIT 30
    `) as unknown as Array<ChunkRow & { snippet?: string }>;

    fusedResults = results.map((r, idx) => ({
      ...r,
      keywordRank: idx + 1,
      semanticRank: 9999,
      keywordScore: Number(r.score),
      semanticScore: 0,
      rrfScore: 1 / (RRF_K + idx + 1),
      rank: Number(r.score),
      snippet: r.snippet || "",
    }));

    suggestSemanticFallback = results.length === 0;

    if (results.length === 0 && processed.forKeyword.length > 3) {
      try {
        await db.execute(sql`CREATE EXTENSION IF NOT EXISTS pg_trgm;`);
        const suggestionRes = await db.execute(sql`
          SELECT word FROM ts_stat(
            'SELECT tsvector_content FROM file_chunks WHERE file_id IN (
              SELECT id FROM files WHERE owner_id = ''${sql.raw(userId)}''
            )'
          )
          WHERE similarity(word, ${processed.forKeyword}) > 0.4
          ORDER BY similarity(word, ${processed.forKeyword}) DESC
          LIMIT 1
        `) as unknown as Array<{ word: string }>;
        if (suggestionRes && suggestionRes.length > 0) {
          spellSuggestion = suggestionRes[0].word;
        }
      } catch (err) {
        console.warn("[RetrievalService] Spelling suggestion query failed:", err);
      }
    }

  } else if (mode === "semantic") {
    // Pure semantic path
    const vectorLiteral = `[${queryEmbedding.join(",")}]`;
    const semanticConditions = [...baseConditions];
    if (queryTerm.length > 1) {
      semanticConditions.push(sql`((1 - (${fileChunks.embedding} <=> ${vectorLiteral}::vector)) > 0.35 OR ${files.originalName} ILIKE ${'%' + queryTerm + '%'})`);
    } else {
      semanticConditions.push(sql`(1 - (${fileChunks.embedding} <=> ${vectorLiteral}::vector)) > 0.35`);
    }
    const semanticScoreExpr = sql`greatest(0, 1 - (${fileChunks.embedding} <=> ${vectorLiteral}::vector))`;

    const semanticHeadlineExpr = generateSnippets
      ? sql`, ts_headline('english', ${fileChunks.chunkText}, ${searchQuery}, 'StartSel=<mark>, StopSel=</mark>, MaxWords=150, MinWords=50') AS "snippet"`
      : sql``;

    const results = await db.execute(sql`
      SELECT
        ${fileChunks.id} AS "chunkId",
        ${fileChunks.fileId} AS "fileId",
        ${fileChunks.chunkIndex} AS "chunkIndex",
        ${fileChunks.chunkText} AS "chunkText",
        ${files.originalName} AS "fileName",
        ${files.mimeType} AS "mimeType",
        ${files.sizeBytes} AS "sizeBytes",
        ${files.tags} AS "tags",
        ${files.summary} AS "summary",
        ${files.createdAt} AS "createdAt",
        (${semanticScoreExpr}) AS "score"
        ${semanticHeadlineExpr}
      FROM ${fileChunks}
      INNER JOIN ${files} ON ${fileChunks.fileId} = ${files.id}
      WHERE ${and(...semanticConditions)}
      ORDER BY ${fileChunks.embedding} <=> ${vectorLiteral}::vector ASC
      LIMIT 30
    `) as unknown as Array<ChunkRow & { snippet?: string }>;

    fusedResults = results.map((r, idx) => ({
      ...r,
      keywordRank: 9999,
      semanticRank: idx + 1,
      keywordScore: 0,
      semanticScore: Number(r.score),
      rrfScore: 1 / (RRF_K + idx + 1),
      rank: Number(r.score),
      snippet: r.snippet || "",
    }));

  } else {
    // === HYBRID MODE: Two-pass Reciprocal Rank Fusion ===

    // Pass 1: Keyword retrieval
    const keywordConditions = [...baseConditions];
    if (queryTerm.length > 1) {
      keywordConditions.push(sql`(${fileChunks.tsvectorContent} @@ ${searchQuery} OR ${files.originalName} ILIKE ${'%' + queryTerm + '%'})`);
    } else {
      keywordConditions.push(sql`${fileChunks.tsvectorContent} @@ ${searchQuery}`);
    }
    const keywordScoreExpr = sql`ts_rank(${fileChunks.tsvectorContent}, ${searchQuery}, 32)`;

    const keywordResults = await db.execute(sql`
      SELECT
        ${fileChunks.id} AS "chunkId",
        ${fileChunks.fileId} AS "fileId",
        ${fileChunks.chunkIndex} AS "chunkIndex",
        ${fileChunks.chunkText} AS "chunkText",
        ${files.originalName} AS "fileName",
        ${files.mimeType} AS "mimeType",
        ${files.sizeBytes} AS "sizeBytes",
        ${files.tags} AS "tags",
        ${files.summary} AS "summary",
        ${files.createdAt} AS "createdAt",
        (${keywordScoreExpr}) AS "score"
      FROM ${fileChunks}
      INNER JOIN ${files} ON ${fileChunks.fileId} = ${files.id}
      WHERE ${and(...keywordConditions)}
      ORDER BY (${keywordScoreExpr}) DESC
      LIMIT 30
    `) as unknown as ChunkRow[];

    // Pass 2: Semantic retrieval
    const vectorLiteral = `[${queryEmbedding.join(",")}]`;
    const semanticConditions = [...baseConditions];
    if (queryTerm.length > 1) {
      semanticConditions.push(sql`((1 - (${fileChunks.embedding} <=> ${vectorLiteral}::vector)) > 0.35 OR ${files.originalName} ILIKE ${'%' + queryTerm + '%'})`);
    } else {
      semanticConditions.push(sql`(1 - (${fileChunks.embedding} <=> ${vectorLiteral}::vector)) > 0.35`);
    }
    const semanticScoreExpr = sql`greatest(0, 1 - (${fileChunks.embedding} <=> ${vectorLiteral}::vector))`;

    const semanticResults = await db.execute(sql`
      SELECT
        ${fileChunks.id} AS "chunkId",
        ${fileChunks.fileId} AS "fileId",
        ${fileChunks.chunkIndex} AS "chunkIndex",
        ${fileChunks.chunkText} AS "chunkText",
        ${files.originalName} AS "fileName",
        ${files.mimeType} AS "mimeType",
        ${files.sizeBytes} AS "sizeBytes",
        ${files.tags} AS "tags",
        ${files.summary} AS "summary",
        ${files.createdAt} AS "createdAt",
        (${semanticScoreExpr}) AS "score"
      FROM ${fileChunks}
      INNER JOIN ${files} ON ${fileChunks.fileId} = ${files.id}
      WHERE ${and(...semanticConditions)}
      ORDER BY ${fileChunks.embedding} <=> ${vectorLiteral}::vector ASC
      LIMIT 30
    `) as unknown as ChunkRow[];

    // Build keyword rank map
    const keywordRankMap = new Map<string, { rank: number; score: number }>();
    keywordResults.forEach((r, idx) => {
      keywordRankMap.set(r.chunkId, { rank: idx + 1, score: Number(r.score) });
    });

    // Build semantic rank map
    const semanticRankMap = new Map<string, { rank: number; score: number }>();
    semanticResults.forEach((r, idx) => {
      semanticRankMap.set(r.chunkId, { rank: idx + 1, score: Number(r.score) });
    });

    // Merge unique chunks from both passes
    const allChunkIds = new Set<string>();
    const chunkDataMap = new Map<string, ChunkRow>();

    for (const r of keywordResults) {
      allChunkIds.add(r.chunkId);
      chunkDataMap.set(r.chunkId, r);
    }
    for (const r of semanticResults) {
      allChunkIds.add(r.chunkId);
      if (!chunkDataMap.has(r.chunkId)) {
        chunkDataMap.set(r.chunkId, r);
      }
    }

    // Compute RRF score for each unique chunk
    for (const chunkId of allChunkIds) {
      const data = chunkDataMap.get(chunkId)!;
      const kw = keywordRankMap.get(chunkId);
      const sem = semanticRankMap.get(chunkId);

      const kwRank = kw ? kw.rank : 9999;
      const semRank = sem ? sem.rank : 9999;
      const kwScore = kw ? kw.score : 0;
      const semScore = sem ? sem.score : 0;

      // Score-weighted RRF using environment-controlled precision weights
      const weightedKw = kw ? (kwScore * env.HYBRID_KEYWORD_WEIGHT) / (RRF_K + kwRank) : 0;
      const weightedSem = sem ? (semScore * env.HYBRID_SEMANTIC_WEIGHT) / (RRF_K + semRank) : 0;
      const rrfScore = weightedKw + weightedSem;

      fusedResults.push({
        ...data,
        keywordRank: kwRank,
        semanticRank: semRank,
        keywordScore: kwScore,
        semanticScore: semScore,
        rrfScore,
        rank: rrfScore,
        snippet: "",
      });
    }

    // Sort by RRF score descending
    fusedResults.sort((a, b) => (b.rrfScore ?? 0) - (a.rrfScore ?? 0));

    // Take top 30 candidates
    fusedResults = fusedResults.slice(0, 30);

    suggestSemanticFallback = keywordResults.length === 0 && semanticResults.length > 0;

    // Generate snippets securely for top candidates if requested
    if (generateSnippets && fusedResults.length > 0) {
      const chunkIds = fusedResults.map((c) => c.chunkId);
      const snippetResults = await db.execute(sql`
        SELECT
          id AS "chunkId",
          ts_headline('english', chunk_text, ${searchQuery}, 'StartSel=<mark>, StopSel=</mark>, MaxWords=150, MinWords=50') AS "snippet"
        FROM file_chunks
        WHERE id IN (${sql.join(chunkIds.map(id => sql`${id}`), sql`, `)})
      `) as unknown as Array<{ chunkId: string; snippet: string }>;

      const snippetMap = new Map<string, string>();
      for (const s of snippetResults) {
        snippetMap.set(s.chunkId, s.snippet);
      }

      for (const candidate of fusedResults) {
        candidate.snippet = snippetMap.get(candidate.chunkId) || "";
      }
    }
  }

  // Unified snippet highlight validation and fallback centering
  if (generateSnippets) {
    for (const candidate of fusedResults) {
      const text = candidate.chunkText || "";
      if (!candidate.snippet || !candidate.snippet.includes("<mark>")) {
        if (text.length <= 300) {
          candidate.snippet = text;
        } else {
          const middle = Math.floor(text.length / 2);
          const start = Math.max(0, middle - 150);
          const end = Math.min(text.length, middle + 150);
          candidate.snippet = (start > 0 ? "..." : "") + text.slice(start, end).trim() + (end < text.length ? "..." : "");
        }
      }
    }
  }

  dbSearchEnd = performance.now();

  // Cross-Encoder Reranking (on semantic and hybrid modes)
  if (fusedResults.length > 0 && (mode === "semantic" || mode === "hybrid")) {
    rerankStart = performance.now();
    try {
      // 1. Deduplicate by fileId (keeping the best chunk per file) before reranking
      const uniqueFileResults: FusedResult[] = [];
      const seenFileIds = new Set<string>();
      for (const res of fusedResults) {
        if (!seenFileIds.has(res.fileId)) {
          seenFileIds.add(res.fileId);
          uniqueFileResults.push(res);
        }
      }

      // 2. Select only the top 10 unique-file chunks to rerank
      const candidatesToRerank = uniqueFileResults.slice(0, 10);

      const cleanDocs = candidatesToRerank.map((res) => {
        const text = res.chunkText || "";
        return text.replace(/<mark>/g, "").replace(/<\/mark>/g, "");
      });

      let rerankResponse;
      try {
        rerankResponse = await fetch("http://embedding:8000/rerank", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ query: processed.forSemantic, documents: cleanDocs }),
        });
      } catch (firstErr) {
        rerankResponse = await fetch("http://localhost:8000/rerank", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ query: processed.forSemantic, documents: cleanDocs }),
        });
      }

      if (rerankResponse.ok) {
        const rerankData = (await rerankResponse.json()) as {
          results: Array<{ index: number; score: number }>;
        };

        // Create a map of candidate chunkId to score
        const rerankScoreMap = new Map<string, number>();
        for (const r of rerankData.results) {
          const candidate = candidatesToRerank[r.index];
          if (candidate) {
            const sigmoidScore = 1 / (1 + Math.exp(-(r.score + 2.0)));
            rerankScoreMap.set(candidate.chunkId, sigmoidScore);
          }
        }

        // Apply rerank scores back to fusedResults
        for (const item of fusedResults) {
          const score = rerankScoreMap.get(item.chunkId);
          if (score !== undefined) {
            item.rank = score;
          } else {
            // Chunks that were not sent to cross-encoder get a low rank to let reranked items bubble up
            item.rank = 0.01;
          }
        }

        fusedResults.sort((a, b) => b.rank - a.rank);
      }
    } catch (err) {
      if (log) {
        log.warn(err, "Cross-encoder reranking failed. Falling back to default scores.");
      } else {
        console.warn("[RetrievalService] Cross-encoder reranking failed. Falling back to default scores:", err);
      }
    }
    rerankEnd = performance.now();
  }

  // Filename fuzzy boost
  const cleanWords = processed.forKeyword.toLowerCase().split(/\s+/).filter(w => w.length > 1);
  for (const res of fusedResults) {
    const lowerName = res.fileName.toLowerCase();
    const lowerQuery = query.toLowerCase();

    let boost = 0;
    if (lowerName.includes(lowerQuery)) {
      boost = 3.0; // massive boost for exact query match in filename
    } else {
      // Check for individual matching words in the filename (fuzzy matches)
      for (const word of cleanWords) {
        if (lowerName.includes(word)) {
          boost += 0.8;
        }
      }
    }
    res.rank = res.rank + Math.min(boost, 1.0);
  }

  // Re-sort based on boosted ranking
  fusedResults.sort((a, b) => b.rank - a.rank);

  const totalEnd = performance.now();

  return {
    results: fusedResults,
    queryEmbedding,
    mode,
    suggestSemanticFallback,
    spellSuggestion,
    timings: {
      embedding: embeddingStart ? embeddingEnd - embeddingStart : 0,
      dbSearch: dbSearchEnd - dbSearchStart,
      rerank: rerankStart ? rerankEnd - rerankStart : 0,
      total: totalEnd - totalStart,
    },
  };
}
