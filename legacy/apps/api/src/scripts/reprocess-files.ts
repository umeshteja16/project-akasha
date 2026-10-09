/**
 * AKASHA File Reprocessing Script
 * Fetches all files from the database and enqueues them back into the BullMQ extraction queue
 * to trigger re-extraction, sentence-boundary chunking, and 768d vector embedding generation.
 *
 * Usage:
 * npx tsx src/scripts/reprocess-files.ts
 */
import { db } from "../db.js";
import { files } from "@akasha/db";
import { extractionQueue } from "../services/queue.service.js";
import pino from "pino";
import * as dotenv from "dotenv";

dotenv.config();

const logger = pino({ level: "info" });

async function reprocess() {
  logger.info("=== Starting AKASHA File Reprocessing & Re-embedding ===");

  // 1. Fetch all files from the database
  const allFiles = await db
    .select({
      id: files.id,
      originalName: files.originalName,
      status: files.status,
    })
    .from(files);

  const totalFiles = allFiles.length;
  logger.info(`Found ${totalFiles} total files in database to reprocess.`);

  if (totalFiles === 0) {
    logger.info("No files found in database. Exiting.");
    process.exit(0);
  }

  // 2. Add each file back to the extraction queue
  let enqueued = 0;
  for (const file of allFiles) {
    try {
      // Add job to the 'file-extraction' queue
      await extractionQueue.add("extract-file", { fileId: file.id });
      enqueued++;
      logger.info(`[${enqueued}/${totalFiles}] Enqueued file: ${file.originalName} (${file.id})`);
    } catch (err: any) {
      logger.error({ error: err.message, fileId: file.id }, `Failed to enqueue file: ${file.originalName}`);
    }
  }

  logger.info(`Successfully enqueued ${enqueued}/${totalFiles} files for processing.`);
  logger.info("The BullMQ background workers will now extract, chunk, and embed these files.");
  
  // Close Redis connection gracefully
  await extractionQueue.close();
  process.exit(0);
}

reprocess().catch((err) => {
  logger.error(err, "Reprocessing script failed!");
  process.exit(1);
});
