import { Worker } from "bullmq";
import { queueRedisConnection } from "./services/queue.service.js";
import { db } from "./db.js";
import { files, fileExtractions, fileChunks, eq, sql } from "@akasha/db";
import { isSupportedMimeType } from "@akasha/shared";
import fs from "fs";
import pdf from "pdf-parse";
import Tesseract from "tesseract.js";
import pino from "pino";
import * as dotenv from "dotenv";
import crypto from "crypto";

dotenv.config();

const logger = pino({
  level: process.env.NODE_ENV === "development" ? "debug" : "info",
});

logger.info("Initializing AKASHA Background Extraction Worker...");

// Sentence-boundary-aware chunker for high-quality semantic segments
// Targets ~2000 chars (~500 tokens) per chunk with 2-3 sentence overlap
function splitIntoSentences(text: string): string[] {
  // Split on sentence-ending punctuation followed by whitespace and a capital letter,
  // or on double newlines (paragraph breaks), or on single newlines followed by bullet markers
  const raw = text.split(/(?<=[.!?])\s+(?=[A-Z])|(?:\n\s*\n)+|(?:\n(?=[\-\*•]\s))/);
  const sentences: string[] = [];
  for (const segment of raw) {
    const trimmed = segment.trim();
    if (!trimmed) continue;
    // If a single "sentence" is extremely long (>3000 chars), force-split it
    if (trimmed.length > 3000) {
      let pos = 0;
      while (pos < trimmed.length) {
        // Try to split at last space before 3000-char boundary
        let end = Math.min(pos + 3000, trimmed.length);
        if (end < trimmed.length) {
          const lastSpace = trimmed.lastIndexOf(" ", end);
          if (lastSpace > pos + 1000) {
            end = lastSpace;
          }
        }
        sentences.push(trimmed.slice(pos, end).trim());
        pos = end;
      }
    } else {
      sentences.push(trimmed);
    }
  }
  return sentences;
}

function generateChunks(text: string, targetSize = 2000, overlapSentences = 3): string[] {
  const chunks: string[] = [];
  if (!text || !text.trim()) return chunks;

  const sentences = splitIntoSentences(text);
  if (sentences.length === 0) return chunks;

  // If total text is small enough for a single chunk, return it directly
  const totalLength = sentences.reduce((sum, s) => sum + s.length, 0);
  if (totalLength <= targetSize * 1.3) {
    chunks.push(sentences.join(" "));
    return chunks;
  }

  let currentChunk: string[] = [];
  let currentLength = 0;

  for (let i = 0; i < sentences.length; i++) {
    const sentence = sentences[i];
    const addedLength = sentence.length + (currentChunk.length > 0 ? 1 : 0); // +1 for space

    if (currentLength + addedLength > targetSize && currentChunk.length > 0) {
      // Close current chunk
      chunks.push(currentChunk.join(" "));

      // Start new chunk with overlap: carry over last N sentences from previous chunk
      const overlapStart = Math.max(0, currentChunk.length - overlapSentences);
      const overlapSegments = currentChunk.slice(overlapStart);
      currentChunk = [...overlapSegments];
      currentLength = overlapSegments.reduce((sum, s) => sum + s.length + 1, 0);
    }

    currentChunk.push(sentence);
    currentLength += addedLength;
  }

  // Flush remaining sentences
  if (currentChunk.length > 0) {
    const lastChunk = currentChunk.join(" ");
    // Avoid tiny trailing chunks — merge with previous if very small
    if (chunks.length > 0 && lastChunk.length < targetSize * 0.25) {
      chunks[chunks.length - 1] += " " + lastChunk;
    } else {
      chunks.push(lastChunk);
    }
  }

  return chunks;
}

const worker = new Worker(
  "file-extraction",
  async (job) => {
    const { fileId } = job.data as { fileId: string };
    
    logger.info({ action: "EXTRACTION_JOB_START", fileId, jobId: job.id }, "Starting file extraction job");

    // 1. Retrieve the file metadata record
    const [fileRecord] = await db
      .select()
      .from(files)
      .where(eq(files.id, fileId))
      .limit(1);

    if (!fileRecord) {
      logger.error({ action: "EXTRACTION_JOB_ERROR", fileId }, "File metadata not found in database");
      throw new Error(`File with ID ${fileId} not found.`);
    }

    // 2. Set the database status to processing
    await db
      .update(files)
      .set({
        status: "processing",
        updatedAt: new Date(),
      })
      .where(eq(files.id, fileId));

    try {
      let extractedText = "";
      const startTime = Date.now();

      // Verify file access on local storage
      await fs.promises.access(fileRecord.storagePath);

      // 3. Process extraction based on MIME type
      const isMedia = fileRecord.mimeType.startsWith("video/") || fileRecord.mimeType.startsWith("audio/");
      const isLargeFile = fileRecord.sizeBytes > 100 * 1024 * 1024;
      const isUnsupported = !isSupportedMimeType(fileRecord.mimeType);

      if (isLargeFile) {
        const sizeMb = (fileRecord.sizeBytes / (1024 * 1024)).toFixed(2);
        const uploadDate = fileRecord.createdAt instanceof Date ? fileRecord.createdAt.toISOString() : new Date(fileRecord.createdAt).toISOString();
        extractedText = `Metadata for large archived file:
Title: ${fileRecord.originalName}
File Size: ${sizeMb} MB
Type: ${fileRecord.mimeType}
Upload Timestamp: ${uploadDate}

This is a large cataloged file titled "${fileRecord.originalName}" with a size of ${sizeMb} MB, registered in the secure retriever. Content processing is skipped due to size limits (>100MB) to preserve resources. It remains retrievable by metadata.`;
      } else if (
        fileRecord.mimeType === "text/plain" ||
        fileRecord.mimeType === "text/markdown" ||
        fileRecord.mimeType === "text/x-markdown" ||
        fileRecord.mimeType === "text/csv"
      ) {
        extractedText = await fs.promises.readFile(fileRecord.storagePath, "utf-8");
      } else if (fileRecord.mimeType === "application/pdf") {
        const pdfBuffer = await fs.promises.readFile(fileRecord.storagePath);
        const parsedPdf = await pdf(pdfBuffer);
        extractedText = parsedPdf.text || "";
      } else if (
        fileRecord.mimeType === "image/jpeg" ||
        fileRecord.mimeType === "image/png" ||
        fileRecord.mimeType === "image/webp"
      ) {
        // Run OCR inside the sandbox/container using Tesseract.js WASM
        const ocrResult = await Tesseract.recognize(fileRecord.storagePath, "eng");
        extractedText = ocrResult.data.text || "";
      } else if (isMedia) {
        const mediaKind = fileRecord.mimeType.startsWith("video/") ? "video" : "audio";
        const sizeMb = (fileRecord.sizeBytes / (1024 * 1024)).toFixed(2);
        const uploadDate = fileRecord.createdAt instanceof Date ? fileRecord.createdAt.toISOString() : new Date(fileRecord.createdAt).toISOString();
        extractedText = `Metadata for ${mediaKind} file:
Title: ${fileRecord.originalName}
File Size: ${sizeMb} MB
Type: ${fileRecord.mimeType}
Upload Timestamp: ${uploadDate}

This is a high-fidelity ${mediaKind} file titled "${fileRecord.originalName}" with a size of ${sizeMb} MB, indexed in the content-addressed retriever. It is accessible for interactive play and retrieval.`;
      } else {
        // Unsupported format: index only filename and metadata
        const sizeMb = (fileRecord.sizeBytes / (1024 * 1024)).toFixed(2);
        const uploadDate = fileRecord.createdAt instanceof Date ? fileRecord.createdAt.toISOString() : new Date(fileRecord.createdAt).toISOString();
        extractedText = `Metadata for unsupported file format:
Title: ${fileRecord.originalName}
File Size: ${sizeMb} MB
Type: ${fileRecord.mimeType}
Upload Timestamp: ${uploadDate}

This is a file titled "${fileRecord.originalName}" in an unsupported format (${fileRecord.mimeType}) with a size of ${sizeMb} MB. Text content cannot be extracted natively. It remains retrievable by metadata.`;
      }

      // Sane fallback if the document represents a blank or empty body
      if (!extractedText.trim()) {
        extractedText = "[No readable text content found in file]";
      }

      const durationMs = Date.now() - startTime;

      // 4. Calculate detailed document stats
      const metadata = {
        pages: fileRecord.mimeType === "application/pdf" ? (extractedText.match(/\f/g) || []).length + 1 : 1,
        wordCount: extractedText.split(/\s+/).filter(Boolean).length,
        durationMs,
        engine: isLargeFile ? "Large File Indexer" : (isUnsupported ? "Unsupported Format Indexer" : (isMedia ? "Media Metadata Analyzer" : (fileRecord.mimeType.startsWith("image/") ? "Tesseract OCR (WASM)" : "Native Text Parser"))),
        mediaType: isMedia ? (fileRecord.mimeType.startsWith("video/") ? "video" : "audio") : undefined,
      };

      // Check if there is an existing extraction record (e.g. for retried jobs)
      const [existingRecord] = await db
        .select()
        .from(fileExtractions)
        .where(eq(fileExtractions.fileId, fileId))
        .limit(1);

      if (existingRecord) {
        await db
          .update(fileExtractions)
          .set({
            extractedText,
            meta: metadata,
            updatedAt: new Date(),
          })
          .where(eq(fileExtractions.fileId, fileId));
      } else {
        await db.insert(fileExtractions).values({
          fileId,
          extractedText,
          meta: metadata,
        });
      }

      // 4.5. Delete existing chunks associated with this fileId to maintain idempotency
      await db.delete(fileChunks).where(eq(fileChunks.fileId, fileId));

      // Determine adaptive chunk target size based on mimeType and file content length
      let targetChunkSize = 2000;
      let overlapCount = 3;
      
      const textLen = extractedText.length;
      if (fileRecord.mimeType === "application/pdf") {
        targetChunkSize = 1500; // Larger to preserve semantic document layout
        overlapCount = 4;
      } else if (fileRecord.mimeType.startsWith("image/")) {
        targetChunkSize = 500; // Smaller for OCR noise
        overlapCount = 1;
      } else if (textLen < 600) {
        targetChunkSize = 1000; // Single chunk
        overlapCount = 0;
      }

      // Generate new chunks and insert them into the Postgres database with dynamically generated tsvectors and embeddings
      const textSegments = generateChunks(extractedText, targetChunkSize, overlapCount);
      let embeddings: number[][] = [];

      if (textSegments.length > 0) {
        try {
          // Calculate checksums for all segments
          const checksums = textSegments.map(segment => 
            crypto.createHash("sha256").update(segment).digest("hex")
          );

          // Find existing embeddings in the database for the same user
          // to reuse them and prevent duplicate embedding generation!
          const existingEmbeds = await db.execute(sql`
            SELECT fc.embedding_checksum AS "checksum", fc.embedding AS "embedding"
            FROM ${fileChunks} fc
            INNER JOIN ${files} f ON fc.file_id = f.id
            WHERE f.owner_id = ${fileRecord.ownerId}
              AND fc.embedding_checksum IN (${sql.raw(checksums.map(c => `'${c}'`).join(','))})
              AND fc.embedding IS NOT NULL
            LIMIT 100
          `) as unknown as Array<{ checksum: string; embedding: string }>;

          const embedCache = new Map<string, number[]>();
          for (const row of existingEmbeds) {
            try {
              if (typeof row.embedding === "string") {
                embedCache.set(row.checksum, JSON.parse(row.embedding));
              } else if (Array.isArray(row.embedding)) {
                embedCache.set(row.checksum, row.embedding);
              }
            } catch (e) {
              // ignore
            }
          }

          // Segments that actually need embedding generation
          const segmentsToEmbed: string[] = [];
          const segmentToEmbedIndices: number[] = [];

          textSegments.forEach((segment, idx) => {
            const chk = checksums[idx];
            if (!embedCache.has(chk)) {
              segmentsToEmbed.push(segment);
              segmentToEmbedIndices.push(idx);
            }
          });

          let newEmbeddings: number[][] = [];
          if (segmentsToEmbed.length > 0) {
            logger.info({ action: "EMBEDDING_GENERATE_START", fileId, segmentsCount: segmentsToEmbed.length }, "Requesting embeddings for missing text segments");
            const embedResponse = await fetch("http://embedding:8000/embed", {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({ texts: segmentsToEmbed }),
            });

            if (!embedResponse.ok) {
              const errBody = await embedResponse.text();
              throw new Error(`Embedding service returned ${embedResponse.status}: ${errBody}`);
            }

            const resData = (await embedResponse.json()) as { embeddings: number[][] };
            newEmbeddings = resData.embeddings;
          }

          // Merge cached and new embeddings
          let newIdx = 0;
          embeddings = textSegments.map((segment, idx) => {
            const chk = checksums[idx];
            if (embedCache.has(chk)) {
              return embedCache.get(chk)!;
            } else {
              return newEmbeddings[newIdx++];
            }
          });

          logger.info({ 
            action: "EMBEDDING_GENERATE_SUCCESS", 
            fileId,
            totalCount: textSegments.length,
            reusedCount: textSegments.length - segmentsToEmbed.length
          }, "Embeddings resolved successfully");
        } catch (err: any) {
          logger.error({ action: "EMBEDDING_GENERATE_FAILURE", fileId, error: err.message }, "Failed to generate embeddings");
          throw err; // Re-throw to fail the worker job so BullMQ will retry
        }
      }

      if (textSegments.length > 0) {
        await db.insert(fileChunks).values(
          textSegments.map((segment, idx) => ({
            fileId,
            chunkIndex: idx,
            chunkText: segment,
            tsvectorContent: sql`to_tsvector('english', ${segment})`,
            embedding: embeddings[idx] || null,
            embeddingModel: "all-mpnet-base-v2",
            embeddingDimensions: 768,
            embeddingChecksum: crypto.createHash("sha256").update(segment).digest("hex"),
            embeddedAt: new Date(),
          }))
        );
      }

      // 4.75. AI Auto-tagging and Summarization using Gemini API
      let tagsList: string[] | null = isLargeFile
        ? ["Large File", "Archive"]
        : (isUnsupported
          ? ["Unsupported Format"]
          : (isMedia
            ? (fileRecord.mimeType.startsWith("video/") ? ["Video", "Media"] : ["Audio", "Media"])
            : null));
      let summaryText: string | null = isLargeFile
        ? `Large cataloged file: ${fileRecord.originalName} (${(fileRecord.sizeBytes / (1024 * 1024)).toFixed(2)} MB).`
        : (isUnsupported
          ? `Unsupported format file: ${fileRecord.originalName} (${(fileRecord.sizeBytes / (1024 * 1024)).toFixed(2)} MB).`
          : (isMedia
            ? `Metadata summary for ${fileRecord.originalName} (${(fileRecord.sizeBytes / (1024 * 1024)).toFixed(2)} MB).`
            : null));

      if (process.env.GEMINI_API_KEY && extractedText.trim() && !isLargeFile && !isUnsupported) {
        try {
          logger.info({ action: "WORKER_AI_TAGGING_START", fileId }, "Requesting AI tags and summary from Gemini API");
          const textToAnalyze = extractedText.slice(0, 15000); 

          const systemPrompt = `You are a professional content analysis engine.
Analyze the provided document text and return a JSON object with exactly two keys:
1. "tags": An array of up to 3 high-quality, conceptual, mixed-case category tags. Do NOT use uppercase shouting (e.g. use "Machine Learning" instead of "MACHINE LEARNING"). Keep them short.
2. "summary": A concise, one-sentence, premium summary of the document. Do not exceed 25 words.

Example JSON output:
{
  "tags": ["Machine Learning", "Neural Networks", "PDF"],
  "summary": "This document outlines the core architectural and mathematical principles of Deep Residual Learning."
}`;

          const userPrompt = `DOCUMENT TEXT TO ANALYZE:
${textToAnalyze}

Generate JSON output:`;

          let response = await fetch(
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
                  temperature: 0.2,
                  maxOutputTokens: 500,
                  responseMimeType: "application/json",
                },
              }),
            }
          );

          if (response.status === 429) {
            logger.warn({ action: "WORKER_AI_TAGGING_429_RETRY", fileId }, "Gemini API rate limit 429 hit. Waiting 30 seconds to retry once...");
            await new Promise((resolve) => setTimeout(resolve, 30000));
            response = await fetch(
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
                    temperature: 0.2,
                    maxOutputTokens: 500,
                    responseMimeType: "application/json",
                  },
                }),
              }
            );
          }

          if (response.ok) {
            const data = (await response.json()) as any;
            const textResponse = data?.candidates?.[0]?.content?.parts?.[0]?.text;
            if (textResponse) {
              const parsed = JSON.parse(textResponse);
              if (Array.isArray(parsed.tags)) {
                tagsList = parsed.tags.slice(0, 3).map((t: any) => String(t).trim());
              }
              if (typeof parsed.summary === "string") {
                summaryText = parsed.summary.trim();
              }
              logger.info({ action: "WORKER_AI_TAGGING_SUCCESS", fileId, tagsList, summaryText }, "AI tags and summary generated successfully");
            }
          } else {
            const errBody = await response.text();
            logger.warn({ action: "WORKER_AI_TAGGING_API_ERROR", fileId, status: response.status, body: errBody }, "Gemini API tagging call returned an error");
          }
        } catch (err: any) {
          logger.warn({ action: "WORKER_AI_TAGGING_ERROR", fileId, error: err.message }, "Failed to generate AI tags and summary");
        }
      }

      // Offline fallback for summary if still null
      if (!summaryText && extractedText) {
        const sentences = extractedText
          .split(/[.!?\n]+/)
          .map(s => s.trim())
          .filter(s => s.length > 10);
        
        const candidateSummary = sentences.slice(0, 2).join(". ") + ".";
        summaryText = candidateSummary.length > 150 
          ? candidateSummary.slice(0, 147) + "..." 
          : candidateSummary;
          
        if (!summaryText.trim() || summaryText === ".") {
          summaryText = `Text content index for ${fileRecord.originalName}.`;
        }
      }

      // Offline fallback for tags if still null
      if (!tagsList) {
        const fileExt = fileRecord.originalName.split(".").pop()?.toUpperCase() || "";
        const fallbackTags = new Set<string>();
        if (fileExt && fileExt.length < 6) {
          fallbackTags.add(fileExt);
        }
        
        // Fast keyword matcher for standard concepts
        const textLower = extractedText.toLowerCase();
        const techKeywords = ["architecture", "research", "guide", "tutorial", "meeting", "finance", "contract", "invoice", "code", "design", "planning"];
        for (const kw of techKeywords) {
          if (textLower.includes(kw) && fallbackTags.size < 3) {
            fallbackTags.add(kw.charAt(0).toUpperCase() + kw.slice(1));
          }
        }
        
        if (fallbackTags.size === 0) {
          fallbackTags.add("Document");
        }
        tagsList = Array.from(fallbackTags);
      }

      // 5. Set terminal success status
      await db
        .update(files)
        .set({
          status: "completed",
          tags: tagsList,
          summary: summaryText,
          updatedAt: new Date(),
        })
        .where(eq(files.id, fileId));

      logger.info(
        {
          action: "EXTRACTION_JOB_SUCCESS",
          fileId,
          durationMs,
          wordCount: metadata.wordCount,
          status: "success",
        },
        "File extraction and text indexing succeeded"
      );
    } catch (err: any) {
      logger.error(
        {
          action: "EXTRACTION_JOB_FAILURE",
          fileId,
          error: err.message,
          stack: err.stack,
          status: "failed",
        },
        "File extraction and text indexing failed"
      );

      // Update database status to failed
      await db
        .update(files)
        .set({
          status: "failed",
          updatedAt: new Date(),
        })
        .where(eq(files.id, fileId));

      // Re-throw so BullMQ flags the job as failed and executes potential retries
      throw err;
    }
  },
  {
    connection: queueRedisConnection,
    concurrency: 2, // Allow up to 2 concurrent extraction streams
  }
);

logger.info("🚀 Background Worker listening on queue 'file-extraction'");

// Graceful termination handling
const gracefulShutdown = async (signal: string) => {
  logger.info({ signal }, `Received termination signal. Closing queue worker...`);
  try {
    await worker.close();
    logger.info("BullMQ worker connection closed successfully.");
    process.exit(0);
  } catch (err) {
    logger.error(err, "An error occurred during queue worker closing");
    process.exit(1);
  }
};

process.on("SIGTERM", () => gracefulShutdown("SIGTERM"));
process.on("SIGINT", () => gracefulShutdown("SIGINT"));
