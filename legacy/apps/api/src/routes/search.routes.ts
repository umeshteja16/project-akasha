import { FastifyInstance, FastifyPluginOptions } from "fastify";
import { authenticate } from "../middleware/auth.js";
import { files, fileChunks, eq, and, sql } from "@akasha/db";
import { db } from "../db.js";
import { ApiError } from "../errors/api-error.js";
import { z } from "zod";
import { preprocessQuery } from "../services/query.service.js";
import { queueRedisConnection } from "../services/queue.service.js";

const searchQueryParamsSchema = z.object({
  q: z.string().min(1, "Search query is required"),
  page: z.string().optional().transform((val?: string) => (val ? parseInt(val, 10) : 1)),
  limit: z.string().optional().transform((val?: string) => (val ? Math.min(Math.max(parseInt(val, 10), 1), 50) : 10)),
  type: z.enum(["pdf", "image", "text", "media", "all"]).default("all"),
  from: z.string().optional(),
  mode: z.enum(["keyword", "semantic", "hybrid"]).default("hybrid"),
  collectionId: z.string().uuid().optional(),
});

import { retrieveChunks, FusedResult } from "../services/retrieval.service.js";
import { validateEnv } from "@akasha/shared";

const env = validateEnv(process.env);

export async function searchRoutes(fastify: FastifyInstance, options: FastifyPluginOptions) {
  fastify.get(
    "/search",
    {
      preHandler: [authenticate],
      config: {
        rateLimit: {
          max: env.NODE_ENV === "development" ? 10000 : 30,
          timeWindow: "1 minute",
        },
      },
    },
    async (request, reply) => {
      const startTime = performance.now();
      const parseResult = searchQueryParamsSchema.safeParse(request.query);
      if (!parseResult.success) {
        throw new ApiError(parseResult.error.issues[0].message, 400, "BAD_REQUEST");
      }

      let { q, page, limit, type, from, mode, collectionId } = parseResult.data;
      const userId = request.user.sub;

      // Preprocess query for optimal retrieval
      const processed = preprocessQuery(q);

      // 60s user-scoped Redis search caching check
      const cacheKey = `search:${userId}:${q}:${mode}:${type}:${page}:${collectionId || "global"}:v3`;
      try {
        const cached = await queueRedisConnection.get(cacheKey);
        if (cached) {
          request.log.info({ action: "SEARCH_CACHE_HIT", userId, query: q }, "Search result cache hit");
          return reply.code(200).send(JSON.parse(cached));
        }
      } catch (err) {
        request.log.warn(err, "Failed to read from search cache");
      }

      let fromParam = from;
      if (!fromParam && processed.detectedFrom) {
        fromParam = processed.detectedFrom;
      }

      try {
        const retrievalResult = await retrieveChunks({
          userId,
          query: q,
          mode,
          type,
          from: fromParam,
          generateSnippets: true,
          collectionId,
          log: request.log,
        });

        const activeMode = retrievalResult.mode;
        const fusedResults = retrievalResult.results;

        // Parent file deduplication
        const seenFiles = new Set<string>();
        const deduplicatedResults: FusedResult[] = [];

        for (const res of fusedResults) {
          if (!seenFiles.has(res.fileId)) {
            seenFiles.add(res.fileId);
            const fileChunksList = fusedResults.filter((r) => r.fileId === res.fileId);
            res.matchCount = fileChunksList.length;
            res.maxRank = Math.max(...fileChunksList.map((r) => r.rank));
            deduplicatedResults.push(res);
          }
        }

        deduplicatedResults.sort((a, b) => (b.maxRank ?? 0) - (a.maxRank ?? 0));
        const paginatedResults = deduplicatedResults.slice((page - 1) * limit, page * limit);

        // Total count for pagination
        const total = deduplicatedResults.length;
        const total_ms = performance.now() - startTime;

        const telemetry = {
          embedding_ms: retrievalResult.timings.embedding,
          db_search_ms: retrievalResult.timings.dbSearch,
          rerank_ms: retrievalResult.timings.rerank,
          total_ms,
        };

        request.log.info(
          {
            action: "KNOWLEDGE_SEARCH",
            userId,
            query: q,
            processedQuery: processed.forKeyword,
            mode: activeMode,
            resultsCount: paginatedResults.length,
            totalHits: total,
            totalDurationMs: total_ms,
            queryExpanded: processed.wasExpanded,
            status: "success",
          },
          "Knowledge search query executed successfully"
        );

        const responseData = {
          data: {
            results: paginatedResults.map((res) => {
              const fileChunksList = fusedResults.filter((r) => r.fileId === res.fileId);
              return {
                chunkId: res.chunkId,
                fileId: res.fileId,
                fileName: res.fileName,
                mimeType: res.mimeType,
                sizeBytes: res.sizeBytes,
                tags: res.tags || [],
                summary: res.summary || "",
                chunkIndex: res.chunkIndex,
                createdAt: res.createdAt instanceof Date ? res.createdAt.toISOString() : new Date(res.createdAt).toISOString(),
                rank: Number(res.rank),
                snippet: res.snippet || "",
                matchCount: Number(res.matchCount),
                allChunks: fileChunksList.map((c) => ({
                  chunkId: c.chunkId,
                  chunkIndex: c.chunkIndex,
                  snippet: c.snippet || "",
                  rank: Number(c.rank),
                })),
                scoreComponents: {
                  keywordScore: Number(res.keywordScore || 0),
                  semanticScore: Number(res.semanticScore || 0),
                  keywordRank: res.keywordRank || 9999,
                  semanticRank: res.semanticRank || 9999,
                  rrfScore: activeMode === "hybrid" ? Number(res.rrfScore || 0) : null,
                  rerankScore: (activeMode === "semantic" || activeMode === "hybrid") ? Number(res.rank) : null,
                }
              };
            }),
            pagination: {
              total,
              page,
              limit,
            },
            queryInfo: {
              original: q,
              processed: processed.forKeyword,
              wasExpanded: processed.wasExpanded,
              expansions: processed.expansions,
              suggestSemanticFallback: !!retrievalResult.suggestSemanticFallback,
              spellSuggestion: retrievalResult.spellSuggestion || null,
            },
            telemetry: {
              embedding_ms: Math.round(telemetry.embedding_ms * 10) / 10,
              db_search_ms: Math.round(telemetry.db_search_ms * 10) / 10,
              rerank_ms: Math.round(telemetry.rerank_ms * 10) / 10,
              total_ms: Math.round(telemetry.total_ms * 10) / 10,
            }
          },
        };

        // Cache search results in Redis with a 60-second TTL
        try {
          await queueRedisConnection.setex(cacheKey, 60, JSON.stringify(responseData));
        } catch (err) {
          request.log.warn(err, "Failed to write to search cache");
        }

        reply.code(200).send(responseData);
      } catch (err: any) {
        request.log.error(err, `Knowledge search failed for query: ${q} in mode: ${mode}`);
        throw new ApiError(
          err.message || "An unexpected error occurred during search retrieval.",
          err.statusCode || 500,
          "INTERNAL_SERVER_ERROR"
        );
      }
    }
  );
}
