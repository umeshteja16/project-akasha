/**
 * AKASHA Embedding Migration Script
 * Migrates all file_chunks from 384d (all-MiniLM-L6-v2) to 768d (all-mpnet-base-v2)
 *
 * Usage: npx tsx src/scripts/migrate-embeddings.ts
 */
import { db } from "../db.js";
import { fileChunks, sql } from "@akasha/db";
import pino from "pino";
import * as dotenv from "dotenv";

dotenv.config();

const logger = pino({ level: "info" });
const BATCH_SIZE = 50;
const EMBED_URL = process.env.EMBEDDING_URL || "http://localhost:8000";

async function migrate() {
  logger.info("=== AKASHA Embedding Migration: 384d → 768d ===");

  // Step 1: Drop the existing HNSW index
  logger.info("Step 1: Dropping existing HNSW index on file_chunks.embedding...");
  try {
    await db.execute(sql`DROP INDEX IF EXISTS file_chunks_embedding_hnsw_idx`);
    logger.info("HNSW index dropped successfully.");
  } catch (err) {
    logger.warn("No existing HNSW index found or drop failed — continuing.");
  }

  // Step 2: Alter column from vector(384) to vector(768)
  logger.info("Step 2: Altering embedding column from vector(384) to vector(768)...");
  try {
    // First set all embeddings to NULL so the column can be altered
    await db.execute(sql`UPDATE file_chunks SET embedding = NULL`);
    await db.execute(sql`ALTER TABLE file_chunks ALTER COLUMN embedding TYPE vector(768)`);
    logger.info("Column altered to vector(768) successfully.");
  } catch (err: any) {
    logger.error({ error: err.message }, "Failed to alter column. Manual intervention may be needed.");
    throw err;
  }

  // Step 3: Fetch all chunks and re-embed in batches
  logger.info("Step 3: Fetching all chunks for re-embedding...");
  const allChunks = await db
    .select({
      id: fileChunks.id,
      chunkText: fileChunks.chunkText,
    })
    .from(fileChunks);

  const totalChunks = allChunks.length;
  logger.info(`Found ${totalChunks} chunks to re-embed.`);

  if (totalChunks === 0) {
    logger.info("No chunks to migrate. Rebuilding index and exiting.");
    await rebuildIndex();
    return;
  }

  const startTime = Date.now();
  let processed = 0;

  for (let i = 0; i < totalChunks; i += BATCH_SIZE) {
    const batch = allChunks.slice(i, i + BATCH_SIZE);
    const texts = batch.map((c) => c.chunkText);

    try {
      const response = await fetch(`${EMBED_URL}/embed`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ texts }),
      });

      if (!response.ok) {
        const errBody = await response.text();
        throw new Error(`Embedding service returned ${response.status}: ${errBody}`);
      }

      const resData = (await response.json()) as { embeddings: number[][] };

      // Update each chunk with its new 768d embedding
      for (let j = 0; j < batch.length; j++) {
        const chunk = batch[j];
        const embedding = resData.embeddings[j];
        if (embedding) {
          await db.execute(
            sql`UPDATE file_chunks SET 
              embedding = ${sql.raw(`'[${embedding.join(",")}]'::vector(768)`)},
              embedding_model = 'all-mpnet-base-v2',
              embedding_dimensions = 768,
              embedding_checksum = encode(sha256(${chunk.chunkText}::bytea), 'hex'),
              embedded_at = NOW()
            WHERE id = ${chunk.id}`
          );
        }
      }

      processed += batch.length;
      const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
      const rate = (processed / parseFloat(elapsed)).toFixed(1);
      logger.info(`Progress: ${processed}/${totalChunks} chunks (${elapsed}s elapsed, ${rate} chunks/s)`);
    } catch (err: any) {
      logger.error({ error: err.message, batchStart: i }, "Batch embedding failed. Retrying individual chunks...");

      // Fallback: embed one at a time
      for (const chunk of batch) {
        try {
          const singleRes = await fetch(`${EMBED_URL}/embed`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ texts: [chunk.chunkText] }),
          });
          if (singleRes.ok) {
            const singleData = (await singleRes.json()) as { embeddings: number[][] };
            const embedding = singleData.embeddings[0];
            if (embedding) {
              await db.execute(
                sql`UPDATE file_chunks SET 
                  embedding = ${sql.raw(`'[${embedding.join(",")}]'::vector(768)`)},
                  embedding_model = 'all-mpnet-base-v2',
                  embedding_dimensions = 768,
                  embedded_at = NOW()
                WHERE id = ${chunk.id}`
              );
            }
          }
        } catch {
          logger.error(`Failed to embed chunk ${chunk.id} — skipping.`);
        }
        processed++;
      }
    }
  }

  const totalElapsed = ((Date.now() - startTime) / 1000).toFixed(1);
  logger.info(`Step 3 complete: ${processed}/${totalChunks} chunks re-embedded in ${totalElapsed}s.`);

  // Step 4: Rebuild HNSW index
  await rebuildIndex();

  logger.info("=== Migration complete! ===");
}

async function rebuildIndex() {
  logger.info("Step 4: Rebuilding HNSW index on file_chunks.embedding...");
  await db.execute(
    sql`CREATE INDEX IF NOT EXISTS file_chunks_embedding_hnsw_idx ON file_chunks USING hnsw (embedding vector_cosine_ops)`
  );
  logger.info("HNSW index rebuilt successfully.");
}

migrate().catch((err) => {
  logger.error(err, "Migration failed!");
  process.exit(1);
});
