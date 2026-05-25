import { FastifyInstance, FastifyPluginOptions } from "fastify";
import { authenticate } from "../middleware/auth.js";
import { files, fileChunks, conversations, conversationMessages, eq, and, sql, desc } from "@akasha/db";
import { db } from "../db.js";
import { ApiError } from "../errors/api-error.js";
import { z } from "zod";
import { preprocessQuery } from "../services/query.service.js";
import { validateEnv } from "@akasha/shared";

const env = validateEnv(process.env);

import { retrieveChunks } from "../services/retrieval.service.js";

const chatBodySchema = z.object({
  q: z.string().min(1, "Search query is required"),
  mode: z.enum(["keyword", "semantic", "hybrid"]).default("hybrid"),
  conversationId: z.string().uuid().optional(),
  collectionId: z.string().uuid().optional(),
  fileIds: z.array(z.string().uuid()).optional(),
});

// Resilient local synthesizer fallback in case Gemini key is missing or calls fail
function generateLocalSynthesizedAnswer(query: string, results: any[]): string {
  if (results.length === 0) {
    return "I could not find this in your files.";
  }

  let answer = `Based on your indexed files, here's what I found for **"${query}"**:\n\n`;

  results.forEach((res, idx) => {
    const segmentId = idx + 1;
    // Clean and split sentences to extract the core ideas
    const sentences = res.chunkText
      .split(/[.!?\n]+/)
      .map((s: string) => s.trim())
      .filter((s: string) => s.length > 10);
      
    const summaryText = sentences.slice(0, 3).join(". ") + ".";
    
    answer += `* **From ${res.fileName} (Chunk #${res.chunkIndex + 1})**:\n  "${summaryText}" [${segmentId}]\n\n`;
  });

  answer += `\n> [!NOTE]\n> AI synthesis unavailable. Showing relevant passages from your files.`;

  return answer;
}

export async function chatRoutes(fastify: FastifyInstance, options: FastifyPluginOptions) {
  // 1. Get all conversations for current user
  fastify.get(
    "/conversations",
    {
      preHandler: [authenticate],
    },
    async (request, reply) => {
      const userId = request.user.sub;
      try {
        const convList = await db
          .select()
          .from(conversations)
          .where(eq(conversations.userId, userId))
          .orderBy(desc(conversations.updatedAt));

        reply.code(200).send({ data: convList });
      } catch (err: any) {
        throw new ApiError(
          err.message || "Failed to retrieve conversations.",
          500,
          "INTERNAL_SERVER_ERROR"
        );
      }
    }
  );

  // 2. Get messages for a specific conversation
  fastify.get(
    "/conversations/:id/messages",
    {
      preHandler: [authenticate],
    },
    async (request, reply) => {
      const userId = request.user.sub;
      const { id } = request.params as { id: string };

      try {
        // Verify conversation belongs to user
        const [conv] = await db
          .select()
          .from(conversations)
          .where(and(eq(conversations.id, id), eq(conversations.userId, userId)));

        if (!conv) {
          throw new ApiError("Conversation not found", 404, "NOT_FOUND");
        }

        const messages = await db
          .select()
          .from(conversationMessages)
          .where(eq(conversationMessages.conversationId, id))
          .orderBy(conversationMessages.createdAt);

        reply.code(200).send({ data: messages });
      } catch (err: any) {
        if (err instanceof ApiError) throw err;
        throw new ApiError(
          err.message || "Failed to retrieve messages.",
          500,
          "INTERNAL_SERVER_ERROR"
        );
      }
    }
  );

  // 3. Delete a conversation
  fastify.delete(
    "/conversations/:id",
    {
      preHandler: [authenticate],
    },
    async (request, reply) => {
      const userId = request.user.sub;
      const { id } = request.params as { id: string };

      try {
        // Verify ownership
        const [conv] = await db
          .select()
          .from(conversations)
          .where(and(eq(conversations.id, id), eq(conversations.userId, userId)));

        if (!conv) {
          throw new ApiError("Conversation not found", 404, "NOT_FOUND");
        }

        await db.delete(conversations).where(eq(conversations.id, id));

        reply.code(200).send({ data: { success: true } });
      } catch (err: any) {
        if (err instanceof ApiError) throw err;
        throw new ApiError(
          err.message || "Failed to delete conversation.",
          500,
          "INTERNAL_SERVER_ERROR"
        );
      }
    }
  );

  // 4. Grounded multi-turn conversational AI
  fastify.post(
    "/chat/grounded",
    {
      preHandler: [authenticate],
      config: {
        rateLimit: {
          max: env.NODE_ENV === "development" ? 10000 : 20,
          timeWindow: "1 minute",
        },
      },
    },
    async (request, reply) => {
      const startTime = performance.now();
      const parseResult = chatBodySchema.safeParse(request.body);
      if (!parseResult.success) {
        throw new ApiError(parseResult.error.issues[0].message, 400, "BAD_REQUEST");
      }

      let { q, mode, conversationId, collectionId, fileIds } = parseResult.data;
      const userId = request.user.sub;

      // 1. Fetch conversation history user messages for retrieval query context fusion
      let fusedQuery = q;
      if (conversationId) {
        try {
          const prevUserMessages = await db
            .select()
            .from(conversationMessages)
            .where(
              and(
                eq(conversationMessages.conversationId, conversationId),
                eq(conversationMessages.role, "user")
              )
            )
            .orderBy(desc(conversationMessages.createdAt))
            .limit(2);

          if (prevUserMessages.length > 0) {
            // Concatenate in chronological order: oldest to newest
            const historyPieces = prevUserMessages
              .reverse()
              .map((msg) => msg.content.trim())
              .filter(Boolean);
            fusedQuery = [...historyPieces, q].join(" ");
          }
        } catch (err) {
          request.log.warn(err, "Failed to retrieve conversation user history for query fusion");
        }
      }

      // Preprocess query for optimal retrieval
      const processed = preprocessQuery(fusedQuery);

      let embeddingStart = 0, embeddingEnd = 0;
      let dbSearchStart = 0, dbSearchEnd = 0;
      let rerankStart = 0, rerankEnd = 0;
      let generationStart = 0, generationEnd = 0;

      try {
        const retrievalResult = await retrieveChunks({
          userId,
          query: fusedQuery,
          mode,
          generateSnippets: false,
          collectionId,
          fileIds,
          log: request.log,
        });

        // Slice to top 10 candidates for grounded context
        const results = retrievalResult.results.slice(0, 10);
        const queryEmbedding = retrievalResult.queryEmbedding;

        // Map timings for down-stream telemetry logging
        embeddingStart = 0;
        embeddingEnd = retrievalResult.timings.embedding;
        dbSearchStart = 0;
        dbSearchEnd = retrievalResult.timings.dbSearch;
        rerankStart = 0;
        rerankEnd = retrievalResult.timings.rerank;

        // Establish the multi-turn session in the database
        let activeConvId = conversationId;
        if (!activeConvId) {
          const title = q.length > 45 ? q.slice(0, 42) + "..." : q;
          const [newConv] = await db
            .insert(conversations)
            .values({
              userId,
              title,
            })
            .returning();
          activeConvId = newConv.id;
        } else {
          // Update the session's active updatedAt tracker
          await db
            .update(conversations)
            .set({ updatedAt: new Date() })
            .where(eq(conversations.id, activeConvId));
        }

        // Persist the user's prompt turn in the conversation history
        await db.insert(conversationMessages).values({
          conversationId: activeConvId,
          role: "user",
          content: q,
          queryEmbedding: queryEmbedding.length > 0 ? queryEmbedding : null,
        });

        // Retrieve conversation history context
        let chatHistoryStr = "";
        if (activeConvId) {
          const history = await db
            .select()
            .from(conversationMessages)
            .where(eq(conversationMessages.conversationId, activeConvId))
            .orderBy(conversationMessages.createdAt)
            .limit(6);

          chatHistoryStr = history
            .map((msg) => `${msg.role.toUpperCase()}: ${msg.content}`)
            .join("\n\n");
        }

        // Hallucination Guardrail Check (Check the top rerank/hybrid score is >= 0.35)
        const topScore = results.length > 0 ? Number(results[0].rank) : 0;
        
        let groundedAnswer = "";
        let usedModel = "LocalResilientSynthesizer";
        let isRefused = false;
        let verifiedContexts: any[] = [];

        generationStart = performance.now();
        if (results.length === 0 || topScore < 0.35) {
          groundedAnswer = "I could not find this in your files.";
          usedModel = "SystemGuardrail";
          isRefused = true;
        } else {
          // Filter context segments to ensure maximum 3 chunks per unique file for high multi-file diversity
          const seenFileCounts = new Map<string, number>();
          const diverseContexts = results
            .filter((res) => Number(res.rank) >= 0.35)
            .filter((chunk) => {
              const count = seenFileCounts.get(chunk.fileId) || 0;
              if (count >= 3) return false;
              seenFileCounts.set(chunk.fileId, count + 1);
              return true;
            });

          // Take top 10 highly relevant diverse contexts
          verifiedContexts = diverseContexts.slice(0, 10);

          const contextStr = verifiedContexts
            .map((res, index) => {
              return `Segment [${index + 1}]:\nSource File: "${res.fileName}" (Chunk #${res.chunkIndex + 1})\nContent:\n${res.chunkText}\n`;
            })
            .join("\n---\n\n");

          const systemPrompt = `You are Antigravity, the intelligence core of AKASHA (a secure, content-addressed personal search & knowledge system).
Your task is to synthesize a precise, accurate, and completely grounded answer to the user's current query using ONLY the provided text segments.

CRITICAL: Only answer using the document excerpts provided. 
If the answer cannot be found in the excerpts, say exactly: 
"I could not find this in your files." 
Do not use any knowledge outside the provided excerpts.

Strict Constraints:
1. Grounding: Every claim you make must be derived directly from the provided segments. Do not assume, extrapolate, or bring in external training knowledge.
2. Citations: You MUST cite your statements inline using numbering like [1], [2], [3], etc. Each number corresponds exactly to the Segment number (e.g., [1] refers to Segment [1]).
3. Format: Use clean, professional, readable Markdown format. Do not use plain text.
4. Silence: If the provided segments do not contain any information relevant to the user's query, say exactly: "I could not find this in your files." and do not generate a hallucinated response.
5. Absolute citation rule: NEVER output an answer without citing the relevant segment(s) when you retrieve context.`;

          const userPrompt = `${chatHistoryStr ? `CONVERSATION HISTORY:\n${chatHistoryStr}\n\n` : ""}USER QUESTION: "${q}"

PROVIDED CONTEXT SEGMENTS:
${contextStr}

Synthesize your grounded answer:`;

          if (env.STRICT_OFFLINE) {
            groundedAnswer = generateLocalSynthesizedAnswer(q, verifiedContexts);
            usedModel = "LocalResilientSynthesizer (Strict Offline)";
          } else if (process.env.GEMINI_API_KEY) {
            try {
              const response = await fetch(
                `https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key=${process.env.GEMINI_API_KEY}`,
                {
                  method: "POST",
                  headers: {
                    "Content-Type": "application/json",
                  },
                  body: JSON.stringify({
                    contents: [{ parts: [{ text: userPrompt }] }],
                    systemInstruction: { parts: [{ text: systemPrompt }] },
                    generationConfig: {
                      temperature: 0.1,
                      maxOutputTokens: 1000,
                    },
                  }),
                }
              );

              if (!response.ok) {
                const errBody = await response.text();
                throw new Error(`Gemini API returned ${response.status}: ${errBody}`);
              }

              const data = (await response.json()) as any;
              groundedAnswer = data?.candidates?.[0]?.content?.parts?.[0]?.text || "";
              usedModel = "gemini-2.5-flash";

              if (!groundedAnswer) {
                throw new Error("Empty response from Gemini API");
              }
            } catch (err: any) {
              request.log.error(err, "Gemini API call failed. Falling back to local synthesizer.");
              groundedAnswer = generateLocalSynthesizedAnswer(q, verifiedContexts);
              usedModel = "LocalResilientSynthesizer (Fallback)";
            }
          } else {
            // No GEMINI_API_KEY available - offline local synthesis fallback
            groundedAnswer = generateLocalSynthesizedAnswer(q, verifiedContexts);
            usedModel = "LocalResilientSynthesizer (Offline)";
          }
        }
        generationEnd = performance.now();

        const responseCitations = isRefused
          ? []
          : verifiedContexts.map((res, idx) => ({
              id: idx + 1,
              chunkId: res.chunkId,
              fileId: res.fileId,
              fileName: res.fileName,
              chunkIndex: res.chunkIndex,
              snippet: res.chunkText.slice(0, 200) + "...",
              rank: Number(res.rank),
            }));

        const total_ms = performance.now() - startTime;
        const telemetry = {
          embedding_ms: embeddingStart ? embeddingEnd - embeddingStart : 0,
          db_search_ms: dbSearchEnd - dbSearchStart,
          rerank_ms: rerankStart ? rerankEnd - rerankStart : 0,
          generation_ms: generationEnd - generationStart,
          total_ms,
        };

        // Persist the assistant's synthesized response turn
        await db.insert(conversationMessages).values({
          conversationId: activeConvId,
          role: "assistant",
          content: groundedAnswer,
          citations: responseCitations,
          telemetry,
        });

        // Structured Logging
        request.log.info(
          {
            action: "GROUNDED_CHAT_ANSWER",
            userId,
            conversationId: activeConvId,
            query: q,
            processedQuery: processed.forKeyword,
            queryExpanded: processed.wasExpanded,
            mode,
            model: usedModel,
            citationsCount: responseCitations.length,
            isRefused,
            totalDurationMs: total_ms,
            status: "success",
          },
          "Grounded chat answer synthesized successfully"
        );

        reply.code(200).send({
          data: {
            conversationId: activeConvId,
            answer: groundedAnswer,
            citations: responseCitations,
            model: usedModel,
            queryInfo: {
              original: q,
              processed: processed.forKeyword,
              wasExpanded: processed.wasExpanded,
              expansions: processed.expansions,
            },
            telemetry: {
              embedding_ms: Math.round(telemetry.embedding_ms * 10) / 10,
              db_search_ms: Math.round(telemetry.db_search_ms * 10) / 10,
              rerank_ms: Math.round(telemetry.rerank_ms * 10) / 10,
              generation_ms: Math.round(telemetry.generation_ms * 10) / 10,
              total_ms: Math.round(telemetry.total_ms * 10) / 10,
            },
          },
        });
      } catch (err: any) {
        request.log.error(err, `Grounded synthesis failed for query: ${q}`);
        throw new ApiError(
          err.message || "An unexpected error occurred during grounded RAG synthesis.",
          err.statusCode || 500,
          "INTERNAL_SERVER_ERROR"
        );
      }
    }
  );
}
