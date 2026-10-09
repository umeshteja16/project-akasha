import { FastifyInstance, FastifyPluginOptions } from "fastify";
import { authenticate } from "../middleware/auth.js";
import { StorageService } from "../services/storage.service.js";
import { files, fileExtractions, fileChunks, collections, users, eq, and, sql } from "@akasha/db";
import { db } from "../db.js";
import { fileUploadSchema, validateEnv, isSupportedMimeType, verifyFileMagicBytes } from "@akasha/shared";
import { ApiError } from "../errors/api-error.js";
import { extractionQueue, queueRedisConnection } from "../services/queue.service.js";
import fs from "fs";
import { z } from "zod";
import { logActivity } from "../services/audit.service.js";
import { Readable } from "stream";

const patchFileBodySchema = z.object({
  originalName: z.string().min(1, "Original name cannot be empty").optional(),
  isStarred: z.boolean().optional(),
  isPinned: z.boolean().optional(),
  collectionId: z.string().uuid().nullable().optional(),
});

// Helper to clear Redis search query cache for a user (non-blocking SCAN-based)
async function clearSearchCache(userId: string) {
  try {
    const pattern = `search:${userId}:*`;
    let cursor = "0";
    do {
      const [nextCursor, keys] = await queueRedisConnection.scan(
        cursor, "MATCH", pattern, "COUNT", 100
      );
      cursor = nextCursor;
      if (keys.length > 0) {
        await queueRedisConnection.del(...keys);
      }
    } while (cursor !== "0");
  } catch (err) {
    // Ignore cache failures silently
  }
}

// Validate env at module level
const env = validateEnv(process.env);

// Helper to map file extensions to mime-types in case client sends application/octet-stream
function detectMimeType(filename: string, clientMimeType: string): string {
  // If client-provided mime-type is already natively supported, trust it
  if (isSupportedMimeType(clientMimeType)) {
    return clientMimeType;
  }

  // Fallback: Detect mime-type based on file extension
  const ext = filename.split(".").pop()?.toLowerCase();
  switch (ext) {
    case "pdf":
      return "application/pdf";
    case "png":
      return "image/png";
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "webp":
      return "image/webp";
    case "txt":
      return "text/plain";
    case "md":
    case "markdown":
      return "text/markdown";
    case "csv":
      return "text/csv";
    case "mp4":
      return "video/mp4";
    case "webm":
      return "video/webm";
    case "mkv":
      return "video/x-matroska";
    case "mp3":
      return "audio/mpeg";
    case "wav":
      return "audio/wav";
    default:
      return clientMimeType || "application/octet-stream";
  }
}


export async function filesRoutes(fastify: FastifyInstance, options: FastifyPluginOptions) {
  /**
   * POST /api/v1/files
   * Protected multipart upload stream to disk, SHA-256 hash, and DB metadata record.
   * Rate limited: 10 uploads per minute.
   */
  fastify.post(
    "/files",
    {
      preHandler: [authenticate],
      config: {
        rateLimit: {
          max: env.NODE_ENV === "development" ? 10000 : 10,
          timeWindow: "1 minute",
        },
      },
    },
    async (request, reply) => {
      // 1. Retrieve the incoming multipart stream
      const part = await request.file();
      if (!part) {
        throw new ApiError("No file was uploaded.", 400, "BAD_REQUEST");
      }

      // 2. Perform pre-stream validations (MIME and filename)
      const detectedMime = detectMimeType(part.filename, part.mimetype);
      const mimeParse = fileUploadSchema.shape.mimeType.safeParse(detectedMime);
      if (!mimeParse.success) {
        throw new ApiError(mimeParse.error.issues[0].message, 415, "UNSUPPORTED_MEDIA_TYPE");
      }

      const filenameParse = fileUploadSchema.shape.filename.safeParse(part.filename);
      if (!filenameParse.success) {
        throw new ApiError(filenameParse.error.issues[0].message, 400, "INVALID_FILENAME");
      }

      const userId = request.user.sub;
 
      // 2.5 Pre-upload storage limit check (query bytes directly)
      const [userRecord] = await db
        .select({ storageLimit: users.storageLimit })
        .from(users)
        .where(eq(users.id, userId))
        .limit(1);
 
      const storageLimit = userRecord?.storageLimit ?? 52428800; // default 50MB
 
      const [currentUsage] = await db
        .select({
          totalBytes: sql<number>`coalesce(sum(${files.sizeBytes}), 0)::int`,
        })
        .from(files)
        .where(eq(files.ownerId, userId));
 
      const storageUsed = currentUsage?.totalBytes || 0;
 
      if (storageUsed >= storageLimit) {
        throw new ApiError("Your storage limit has been exceeded. Please delete some files or expand your storage limit in Settings.", 400, "STORAGE_LIMIT_EXCEEDED");
      }
 
      try {
        // 3. Stream file directly to local disk
        const uploadResult = await StorageService.saveStream(part.file, env.UPLOAD_DIR);
 
        // 4. Check if Fastify multipart limit was hit (file truncation)
        if (part.file.truncated) {
          // Clean up the partial/truncated physical file
          await StorageService.deleteFile(uploadResult.storagePath);
          throw new ApiError(`File exceeds the maximum size limit of ${env.MAX_FILE_SIZE_MB}MB`, 413, "FILE_TOO_LARGE");
        }
 
        // 4.5 Post-upload exact storage limit check
        if (storageUsed + uploadResult.sizeBytes > storageLimit) {
          await StorageService.deleteFile(uploadResult.storagePath);
          throw new ApiError(
            `Uploading this file would exceed your storage limit. Remaining space: ${Math.max(0, Math.floor((storageLimit - storageUsed) / (1024 * 1024)))}MB.`,
            400,
            "STORAGE_LIMIT_EXCEEDED"
          );
        }

        // 4.7 Magic bytes / signature security verification
        const fd = await fs.promises.open(uploadResult.storagePath, "r");
        try {
          const buffer = Buffer.alloc(16);
          const { bytesRead } = await fd.read(buffer, 0, 16, 0);
          const isValidMagic = verifyFileMagicBytes(
            new Uint8Array(buffer.subarray(0, bytesRead)),
            detectedMime
          );
          if (!isValidMagic) {
            await fd.close();
            // Delete file immediately
            await StorageService.deleteFile(uploadResult.storagePath);
            throw new ApiError(
              "Security Violation: File contents do not match the expected format signature.",
              415,
              "UNSUPPORTED_MEDIA_TYPE"
            );
          }
        } finally {
          await fd.close();
        }
 
        // 5. Insert file metadata record into PostgreSQL
        const [newFile] = await db
          .insert(files)
          .values({
            ownerId: userId,
            originalName: part.filename,
            storagePath: uploadResult.storagePath,
            contentHash: uploadResult.contentHash,
            mimeType: detectedMime,
            sizeBytes: uploadResult.sizeBytes,
            status: "pending",
          })
          .returning();

        // 6. Structured Logging
        request.log.info(
          {
            action: "FILE_UPLOAD",
            userId,
            fileId: newFile.id,
            status: "success",
          },
          "File uploaded successfully"
        );

        // Persistent Audit Logging
        await logActivity({
          userId,
          action: "FILE_UPLOAD",
          fileId: newFile.id,
          ipAddress: request.ip,
        });

        // 7. Enqueue asynchronous extraction and OCR job via BullMQ
        await extractionQueue.add("extract", { fileId: newFile.id });
        request.log.info(
          {
            action: "EXTRACTION_ENQUEUE",
            userId,
            fileId: newFile.id,
            status: "success",
          },
          "Extraction job enqueued successfully"
        );

        // 8. Invalidate search cache
        await clearSearchCache(userId);

        // 9. Return success response format
        reply.code(201).send({
          data: {
            id: newFile.id,
            status: newFile.status,
          },
        });
      } catch (err: any) {
        if (err instanceof ApiError) {
          throw err;
        }
        request.log.error(err, "Error occurred during file upload streaming");
        throw new ApiError(err.message || "An unexpected error occurred while uploading the file.", err.statusCode || 500, "INTERNAL_SERVER_ERROR");
      }
    }
  );

  /**
   * GET /api/v1/files
   * Protected owner-scoped, paginated listing of file metadata.
   */
  fastify.get("/files", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;

    // Parse pagination query parameters
    const query = request.query as Record<string, string | undefined>;
    const limit = Math.min(Math.max(parseInt(query.limit || "20", 10), 1), 100);
    const offset = Math.max(parseInt(query.offset || "0", 10), 0);

    try {
      // Retrieve metadata records belonging to the authenticated user with active chunkCount subquery
      const userFiles = await db
        .select({
          id: files.id,
          originalName: files.originalName,
          mimeType: files.mimeType,
          sizeBytes: files.sizeBytes,
          status: files.status,
          createdAt: files.createdAt,
          tags: files.tags,
          summary: files.summary,
          isStarred: files.isStarred,
          isPinned: files.isPinned,
          openCount: files.openCount,
          lastOpenedAt: files.lastOpenedAt,
          collectionId: files.collectionId,
          chunkCount: sql<number>`(SELECT count(*)::int FROM ${fileChunks} WHERE ${fileChunks.fileId} = ${files.id})`
        })
        .from(files)
        .where(eq(files.ownerId, userId))
        .limit(limit)
        .offset(offset)
        .orderBy(sql`${files.createdAt} DESC`);

      // Map to standard response structure
      const formattedFiles = userFiles.map((file) => ({
        id: file.id,
        originalName: file.originalName,
        mimeType: file.mimeType,
        sizeBytes: file.sizeBytes,
        status: file.status,
        createdAt: file.createdAt.toISOString(),
        tags: file.tags || [],
        summary: file.summary || "",
        isStarred: file.isStarred,
        isPinned: file.isPinned,
        openCount: file.openCount,
        lastOpenedAt: file.lastOpenedAt?.toISOString() || null,
        collectionId: file.collectionId,
        chunkCount: file.chunkCount || 0,
      }));

      // Structured Logging
      request.log.info(
        {
          action: "FILE_LIST",
          userId,
          limit,
          offset,
          status: "success",
        },
        "Files listed successfully"
      );

      reply.code(200).send({
        data: formattedFiles,
      });
    } catch (err: any) {
      if (err instanceof ApiError) {
        throw err;
      }
      request.log.error(err, "Failed to retrieve user files list");
      throw new ApiError(err.message || "An error occurred while fetching the files list.", err.statusCode || 500, "INTERNAL_SERVER_ERROR");
    }
  });

  /**
   * DELETE /api/v1/files/:id
   * Protected owner-scoped file metadata deletion and safe physical storage de-duplication check.
   */
  fastify.delete("/files/:id", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const { id: fileId } = request.params as { id: string };

    try {
      // 1. Find the file metadata record and ensure the user owns it
      const [fileRecord] = await db
        .select()
        .from(files)
        .where(and(eq(files.id, fileId), eq(files.ownerId, userId)))
        .limit(1);

      if (!fileRecord) {
        throw new ApiError("File not found or access denied.", 404, "FILE_NOT_FOUND");
      }

      // 2. Delete database metadata record
      await db.delete(files).where(eq(files.id, fileId));

      // 3. Query the database to check if another record still references this same physical file path
      const [remainingRecord] = await db
        .select()
        .from(files)
        .where(eq(files.storagePath, fileRecord.storagePath))
        .limit(1);

      // 4. Physical cleanup: Delete the file on disk only if no other active references exist
      if (!remainingRecord) {
        await StorageService.deleteFile(fileRecord.storagePath);
      }

      // Structured Logging
      request.log.info(
        {
          action: "FILE_DELETE",
          userId,
          fileId,
          status: "success",
        },
        "File deleted successfully"
      );

      // Persistent Audit Logging
      await logActivity({
        userId,
        action: "FILE_DELETE",
        fileId,
        ipAddress: request.ip,
      });

      // Invalidate search cache
      await clearSearchCache(userId);

      reply.code(200).send({
        success: true,
      });
    } catch (err: any) {
      if (err instanceof ApiError) {
        throw err;
      }
      request.log.error(err, `Failed to delete file with ID: ${fileId}`);
      throw new ApiError(err.message || "An error occurred while deleting the file.", err.statusCode || 500, "INTERNAL_SERVER_ERROR");
    }
  });

  /**
   * PATCH /api/v1/files/:id
   * Protected owner-scoped update of file originalName or isStarred status.
   */
  fastify.patch("/files/:id", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const { id: fileId } = request.params as { id: string };
    const parseResult = patchFileBodySchema.safeParse(request.body);
    if (!parseResult.success) {
      throw new ApiError(parseResult.error.issues[0].message, 400, "BAD_REQUEST");
    }
    const { originalName, isStarred, isPinned, collectionId } = parseResult.data;

    try {
      const [fileRecord] = await db
        .select()
        .from(files)
        .where(and(eq(files.id, fileId), eq(files.ownerId, userId)))
        .limit(1);

      if (!fileRecord) {
        throw new ApiError("File not found or access denied.", 404, "FILE_NOT_FOUND");
      }

      const updateData: Record<string, any> = {
        updatedAt: new Date(),
      };
      if (originalName !== undefined) {
        const filenameParse = fileUploadSchema.shape.filename.safeParse(originalName);
        if (!filenameParse.success) {
          throw new ApiError(filenameParse.error.issues[0].message, 400, "INVALID_FILENAME");
        }
        updateData.originalName = originalName;
      }
      if (isStarred !== undefined) {
        updateData.isStarred = isStarred;
      }
      if (isPinned !== undefined) {
        updateData.isPinned = isPinned;
      }
      if (collectionId !== undefined) {
        updateData.collectionId = collectionId;
      }

      const [updatedFile] = await db
        .update(files)
        .set(updateData)
        .where(eq(files.id, fileId))
        .returning();

      // Invalidate search cache
      await clearSearchCache(userId);

      reply.code(200).send({
        success: true,
        data: {
          id: updatedFile.id,
          originalName: updatedFile.originalName,
          isStarred: updatedFile.isStarred,
          isPinned: updatedFile.isPinned,
          collectionId: updatedFile.collectionId,
        },
      });
    } catch (err: any) {
      if (err instanceof ApiError) throw err;
      throw new ApiError(err.message || "Failed to update file.", 500, "INTERNAL_SERVER_ERROR");
    }
  });

  /**
   * GET /api/v1/files/:id/download
   * Protected owner-scoped file download route.
   * Validates JWT, ownership check, sets content-disposition and content-type, and streams physical file directly.
   */
  fastify.get("/files/:id/download", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const { id: fileId } = request.params as { id: string };

    try {
      // 1. Fetch file metadata record and ensure the user owns it
      const [fileRecord] = await db
        .select()
        .from(files)
        .where(and(eq(files.id, fileId), eq(files.ownerId, userId)))
        .limit(1);

      if (!fileRecord) {
        throw new ApiError("File not found or access denied.", 404, "FILE_NOT_FOUND");
      }

      // 2. Check if the physical file exists on disk
      try {
        await fs.promises.access(fileRecord.storagePath);
      } catch {
        throw new ApiError("The physical file could not be found on disk.", 404, "FILE_NOT_FOUND_ON_DISK");
      }

      // Structured Logging
      request.log.info(
        {
          action: "FILE_DOWNLOAD",
          userId,
          fileId,
          status: "success",
        },
        "File download streaming started"
      );

      // 3. Set proper streaming headers
      reply.header("Content-Type", fileRecord.mimeType);
      // Proper content disposition: attachment to trigger a native download, with original file name
      reply.header(
        "Content-Disposition",
        `attachment; filename="${encodeURIComponent(fileRecord.originalName)}"`
      );

      // 4. Create readable stream and pipe it to reply payload without buffering in memory
      const fileStream = fs.createReadStream(fileRecord.storagePath);
      return reply.send(fileStream);
    } catch (err: any) {
      if (err instanceof ApiError) {
        throw err;
      }
      request.log.error(err, `Failed to download file with ID: ${fileId}`);
      throw new ApiError(err.message || "An error occurred while downloading the file.", err.statusCode || 500, "INTERNAL_SERVER_ERROR");
    }
  });

  /**
   * GET /api/v1/files/:id/extraction
   * Protected owner-scoped retrieval of file extraction text and metadata.
   */
  fastify.get("/files/:id/extraction", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const { id: fileId } = request.params as { id: string };

    try {
      // 1. Fetch file metadata and ensure user owns it
      const [fileRecord] = await db
        .select()
        .from(files)
        .where(and(eq(files.id, fileId), eq(files.ownerId, userId)))
        .limit(1);

      if (!fileRecord) {
        throw new ApiError("File not found or access denied.", 404, "FILE_NOT_FOUND");
      }

      // 2. Fetch the corresponding extraction record
      const [extractionRecord] = await db
        .select()
        .from(fileExtractions)
        .where(eq(fileExtractions.fileId, fileId))
        .limit(1);

      if (!extractionRecord) {
        throw new ApiError("No extraction text available for this file yet.", 404, "EXTRACTION_NOT_FOUND");
      }

      // Structured Logging
      request.log.info(
        {
          action: "FILE_EXTRACTION_GET",
          userId,
          fileId,
          status: "success",
        },
        "File extraction retrieved successfully"
      );

      reply.code(200).send({
        data: {
          fileId: extractionRecord.fileId,
          extractedText: extractionRecord.extractedText,
          meta: extractionRecord.meta,
          createdAt: extractionRecord.createdAt.toISOString(),
          mimeType: fileRecord.mimeType,
          originalName: fileRecord.originalName,
          fileCreatedAt: fileRecord.createdAt.toISOString(),
          tags: fileRecord.tags || [],
          summary: fileRecord.summary || "",
          sizeBytes: fileRecord.sizeBytes,
          status: fileRecord.status,
          collectionId: fileRecord.collectionId,
        },
      });
    } catch (err: any) {
      if (err instanceof ApiError) {
        throw err;
      }
      request.log.error(err, `Failed to retrieve extraction for file ID: ${fileId}`);
      throw new ApiError(
        err.message || "An error occurred while fetching the extraction text.",
        err.statusCode || 500,
        "INTERNAL_SERVER_ERROR"
      );
    }
  });

  /**
   * PUT /api/v1/files/:id/tags
   * Protected owner-scoped update of file tags.
   */
  fastify.put("/files/:id/tags", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const { id: fileId } = request.params as { id: string };
    const { tags } = request.body as { tags: string[] };

    if (!Array.isArray(tags)) {
      throw new ApiError("Tags must be an array of strings", 400, "BAD_REQUEST");
    }

    try {
      const [fileRecord] = await db
        .select()
        .from(files)
        .where(and(eq(files.id, fileId), eq(files.ownerId, userId)))
        .limit(1);

      if (!fileRecord) {
        throw new ApiError("File not found or access denied.", 404, "FILE_NOT_FOUND");
      }

      await db
        .update(files)
        .set({
          tags: tags,
          updatedAt: new Date(),
        })
        .where(eq(files.id, fileId));

      // Invalidate search cache
      await clearSearchCache(userId);

      reply.code(200).send({ success: true, tags });
    } catch (err: any) {
      if (err instanceof ApiError) throw err;
      throw new ApiError(err.message || "Failed to update tags.", 500, "INTERNAL_SERVER_ERROR");
    }
  });

  /**
   * PATCH /api/v1/files/:id/open
   * Protected owner-scoped update of lastOpenedAt and increment of openCount.
   */
  fastify.patch("/files/:id/open", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const { id: fileId } = request.params as { id: string };

    try {
      const [fileRecord] = await db
        .select()
        .from(files)
        .where(and(eq(files.id, fileId), eq(files.ownerId, userId)))
        .limit(1);

      if (!fileRecord) {
        throw new ApiError("File not found or access denied.", 404, "FILE_NOT_FOUND");
      }

      const [updatedFile] = await db
        .update(files)
        .set({
          lastOpenedAt: new Date(),
          openCount: fileRecord.openCount + 1,
          updatedAt: new Date(),
        })
        .where(eq(files.id, fileId))
        .returning();

      reply.code(200).send({
        success: true,
        data: {
          id: updatedFile.id,
          lastOpenedAt: updatedFile.lastOpenedAt?.toISOString() || null,
          openCount: updatedFile.openCount,
        },
      });
    } catch (err: any) {
      if (err instanceof ApiError) throw err;
      throw new ApiError(err.message || "Failed to update open count.", 500, "INTERNAL_SERVER_ERROR");
    }
  });

  /**
   * POST /api/v1/files/:id/reindex
   * Protected owner-scoped re-queuing of file extraction extraction pipelines.
   */
  fastify.post("/files/:id/reindex", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const { id: fileId } = request.params as { id: string };

    try {
      const [fileRecord] = await db
        .select()
        .from(files)
        .where(and(eq(files.id, fileId), eq(files.ownerId, userId)))
        .limit(1);

      if (!fileRecord) {
        throw new ApiError("File not found or access denied.", 404, "FILE_NOT_FOUND");
      }

      // Reset document status to pending
      await db
        .update(files)
        .set({
          status: "pending",
          updatedAt: new Date(),
        })
        .where(eq(files.id, fileId));

      // Re-enqueue BullMQ extraction and OCR job
      await extractionQueue.add("extract", { fileId: fileRecord.id });

      request.log.info(
        {
          action: "FILE_REINDEX",
          userId,
          fileId: fileRecord.id,
          status: "success",
        },
        "File reindex job enqueued successfully"
      );

      // Invalidate search cache
      await clearSearchCache(userId);

      reply.code(200).send({
        success: true,
        message: "Reindex extraction enqueued.",
      });
    } catch (err: any) {
      if (err instanceof ApiError) throw err;
      throw new ApiError(err.message || "Failed to trigger re-index job.", 500, "INTERNAL_SERVER_ERROR");
    }
  });

  /**
   * GET /api/v1/files/:id/similar
   * Protected owner-scoped retrieval of semantically similar files using pgvector.
   */
  fastify.get("/files/:id/similar", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const { id: fileId } = request.params as { id: string };

    try {
      const targetChunks = await db
        .select({ embedding: fileChunks.embedding })
        .from(fileChunks)
        .where(eq(fileChunks.fileId, fileId))
        .limit(5);

      if (targetChunks.length === 0) {
        return reply.code(200).send({ data: [] });
      }

      const validEmbeds = targetChunks.map(c => c.embedding).filter((e): e is number[] => e !== null);
      if (validEmbeds.length === 0) {
        return reply.code(200).send({ data: [] });
      }

      const targetEmbed = validEmbeds[0];
      const vectorLiteral = `[${targetEmbed.join(",")}]`;

      const similarResults = await db.execute(sql`
        WITH ranked AS (
          SELECT DISTINCT ON (f.id)
            f.id AS "id",
            f.original_name AS "originalName",
            f.mime_type AS "mimeType",
            f.size_bytes AS "sizeBytes",
            f.status AS "status",
            f.is_starred AS "isStarred",
            1 - (fc.embedding <=> ${vectorLiteral}::vector) AS "similarity"
          FROM ${fileChunks} fc
          INNER JOIN ${files} f ON fc.file_id = f.id
          WHERE f.owner_id = ${userId}
            AND f.id != ${fileId}
            AND fc.embedding IS NOT NULL
          ORDER BY f.id, 1 - (fc.embedding <=> ${vectorLiteral}::vector) DESC
        )
        SELECT * FROM ranked
        ORDER BY "similarity" DESC
        LIMIT 5
      `) as unknown as Array<{ id: string; originalName: string; mimeType: string; sizeBytes: number; status: string; isStarred: boolean; similarity: number }>;

      reply.code(200).send({ data: similarResults });
    } catch (err: any) {
      request.log.error(err, `Failed to retrieve similar files for ID: ${fileId}`);
      throw new ApiError(err.message || "Failed to retrieve similar files.", 500, "INTERNAL_SERVER_ERROR");
    }
  });

  /**
   * GET /api/v1/files/:id/thumbnail
   * Protected owner-scoped thumbnail endpoint for images.
   * Resizes image to 200x200 via sharp if available, else streams raw image.
   */
  fastify.get("/files/:id/thumbnail", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const { id: fileId } = request.params as { id: string };

    try {
      const [fileRecord] = await db
        .select()
        .from(files)
        .where(and(eq(files.id, fileId), eq(files.ownerId, userId)))
        .limit(1);

      if (!fileRecord) {
        throw new ApiError("File not found.", 404, "FILE_NOT_FOUND");
      }

      if (!fileRecord.mimeType.startsWith("image/")) {
        throw new ApiError("Thumbnails are only supported for images.", 400, "BAD_REQUEST");
      }

      try {
        await fs.promises.access(fileRecord.storagePath);
      } catch {
        throw new ApiError("File not found on disk.", 404, "FILE_NOT_FOUND");
      }

      const thumbPath = `${fileRecord.storagePath}-thumb.webp`;

      try {
        await fs.promises.access(thumbPath); // Check if cached thumbnail exists
        reply.header("Content-Type", "image/webp");
        reply.header("Cache-Control", "public, max-age=86400"); // cache 24h
        return reply.send(fs.createReadStream(thumbPath));
      } catch {
        // Generate and cache
        try {
          const sharpModule = await (eval('import("sharp")') as Promise<any>);
          const sharp = sharpModule.default;
          const resized = await sharp(fileRecord.storagePath)
            .resize(200, 200, { fit: "cover" })
            .webp({ quality: 70 })
            .toBuffer();
          
          await fs.promises.writeFile(thumbPath, resized);
          
          reply.header("Content-Type", "image/webp");
          reply.header("Cache-Control", "public, max-age=86400"); // cache 24h
          return reply.send(resized);
        } catch (err) {
          // Fallback: Stream raw image directly
          reply.header("Content-Type", fileRecord.mimeType);
          reply.header("Cache-Control", "public, max-age=86400"); // cache 24h
          const stream = fs.createReadStream(fileRecord.storagePath);
          return reply.send(stream);
        }
      }
    } catch (err: any) {
      if (err instanceof ApiError) throw err;
      throw new ApiError(err.message || "Failed to generate thumbnail.", 500, "INTERNAL_SERVER_ERROR");
    }
  });

  const bulkDeleteBodySchema = z.object({
    fileIds: z.array(z.string().uuid()).min(1, "At least one file ID is required"),
  });

  /**
   * POST /api/v1/files/bulk-delete
   * Protected owner-scoped bulk deletion of file records and assets.
   */
  fastify.post("/files/bulk-delete", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const parseResult = bulkDeleteBodySchema.safeParse(request.body);
    if (!parseResult.success) {
      throw new ApiError(parseResult.error.issues[0].message, 400, "BAD_REQUEST");
    }
    const { fileIds } = parseResult.data;

    try {
      const fileRecords = await db
        .select()
        .from(files)
        .where(and(sql`${files.id} IN (${sql.join(fileIds.map(id => sql`${id}`), sql`, `)})`, eq(files.ownerId, userId)));

      if (fileRecords.length === 0) {
        return reply.code(200).send({ success: true, count: 0 });
      }

      const validIds = fileRecords.map(f => f.id);

      await db.delete(files).where(sql`${files.id} IN (${sql.join(validIds.map(id => sql`${id}`), sql`, `)})`);

      for (const record of fileRecords) {
        const [remainingRecord] = await db
          .select()
          .from(files)
          .where(eq(files.storagePath, record.storagePath))
          .limit(1);

        if (!remainingRecord) {
          try {
            await StorageService.deleteFile(record.storagePath);
          } catch (e) {
            // Ignore disk deletion errors
          }
        }
      }

      request.log.info({ action: "BULK_FILE_DELETE", userId, count: validIds.length, status: "success" }, "Files bulk deleted successfully");

      for (const fId of validIds) {
        await logActivity({ userId, action: "FILE_DELETE", fileId: fId, ipAddress: request.ip });
      }

      await clearSearchCache(userId);

      reply.code(200).send({ success: true, count: validIds.length });
    } catch (err: any) {
      request.log.error(err, "Failed to bulk delete files");
      throw new ApiError(err.message || "Failed to bulk delete files.", 500, "INTERNAL_SERVER_ERROR");
    }
  });

  /**
   * GET /api/v1/collections
   * Protected owner-scoped list of user collections.
   */
  fastify.get("/collections", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    try {
      const list = await db
        .select()
        .from(collections)
        .where(eq(collections.ownerId, userId))
        .orderBy(sql`${collections.createdAt} DESC`);
      reply.code(200).send({ data: list });
    } catch (err: any) {
      throw new ApiError(err.message || "Failed to list collections", 500, "INTERNAL_SERVER_ERROR");
    }
  });

  const createCollectionSchema = z.object({
    name: z.string().min(1, "Name is required"),
    color: z.string().default("#6c63ff"),
  });

  /**
   * POST /api/v1/collections
   * Protected owner-scoped creation of a collection.
   */
  fastify.post("/collections", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const parseResult = createCollectionSchema.safeParse(request.body);
    if (!parseResult.success) {
      throw new ApiError(parseResult.error.issues[0].message, 400, "BAD_REQUEST");
    }
    const { name, color } = parseResult.data;
    try {
      const [newCol] = await db
        .insert(collections)
        .values({
          ownerId: userId,
          name,
          color,
        })
        .returning();
      reply.code(201).send({ data: newCol });
    } catch (err: any) {
      throw new ApiError(err.message || "Failed to create collection", 500, "INTERNAL_SERVER_ERROR");
    }
  });

  const captureBodySchema = z.object({
    content: z.string().min(1, "Content is required"),
    type: z.enum(["note", "url"]).default("note"),
  });

  /**
   * POST /api/v1/files/capture
   * Protected owner-scoped creation of raw text notes or URL textual extractions.
   */
  fastify.post("/files/capture", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const parseResult = captureBodySchema.safeParse(request.body);
    if (!parseResult.success) {
      throw new ApiError(parseResult.error.issues[0].message, 400, "BAD_REQUEST");
    }
    let { content, type } = parseResult.data;
    let filename = `Note_${new Date().toISOString().replace(/[:.]/g, "-")}.txt`;
    let textToStore = content;

    if (type === "url" || content.startsWith("http://") || content.startsWith("https://")) {
      type = "url";
      const urlString = content.trim();
      filename = `Web_${urlString.replace(/https?:\/\//, "").replace(/[^a-zA-Z0-9.-]/g, "_").slice(0, 50)}.txt`;
      try {
        const fetchRes = await fetch(urlString, {
          headers: {
            "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
          }
        });
        if (!fetchRes.ok) {
          throw new Error(`Web fetch returned status ${fetchRes.status}`);
        }
        const html = await fetchRes.text();
        const textWithoutScripts = html
          .replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, "")
          .replace(/<style\b[^<]*(?:(?!<\/style>)<[^<]*)*<\/style>/gi, "");
        const cleanText = textWithoutScripts
          .replace(/<[^>]*>/g, " ")
          .replace(/\s+/g, " ")
          .trim();
        if (!cleanText) {
          throw new Error("Extracted empty text from the webpage.");
        }
        textToStore = `Source URL: ${urlString}\n\n${cleanText}`;
      } catch (err: any) {
        throw new ApiError(`Failed to capture webpage: ${err.message}`, 400, "WEBPAGE_CAPTURE_FAILED");
      }
    }

    try {
      const stream = Readable.from(textToStore);
      const uploadResult = await StorageService.saveStream(stream, env.UPLOAD_DIR);

      const [newFile] = await db
        .insert(files)
        .values({
          ownerId: userId,
          originalName: filename,
          storagePath: uploadResult.storagePath,
          contentHash: uploadResult.contentHash,
          mimeType: "text/plain",
          sizeBytes: uploadResult.sizeBytes,
          status: "pending",
        })
        .returning();

      // Audit log
      await logActivity({
        userId,
        action: "FILE_UPLOAD",
        fileId: newFile.id,
        ipAddress: request.ip,
      });

      // Enqueue asynchronous extraction and OCR job via BullMQ
      await extractionQueue.add("extract", { fileId: newFile.id });

      // Invalidate search cache
      await clearSearchCache(userId);

      reply.code(201).send({ data: newFile });
    } catch (err: any) {
      throw new ApiError(err.message || "Failed to capture thought", 500, "INTERNAL_SERVER_ERROR");
    }
  });

  /**
   * DELETE /api/v1/collections/:id
   * Protected owner-scoped deletion of a collection.
   */
  fastify.delete("/collections/:id", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const { id: colId } = request.params as { id: string };
    try {
      const [colRecord] = await db
        .select()
        .from(collections)
        .where(and(eq(collections.id, colId), eq(collections.ownerId, userId)))
        .limit(1);

      if (!colRecord) {
        throw new ApiError("Collection not found.", 404, "COLLECTION_NOT_FOUND");
      }

      await db.delete(collections).where(eq(collections.id, colId));
      reply.code(200).send({ success: true });
    } catch (err: any) {
      if (err instanceof ApiError) throw err;
      throw new ApiError(err.message || "Failed to delete collection", 500, "INTERNAL_SERVER_ERROR");
    }
  });

  const updateCollectionSchema = z.object({
    name: z.string().min(1, "Name cannot be empty").optional(),
    color: z.string().optional(),
  });

  /**
   * PATCH /api/v1/collections/:id
   * Protected owner-scoped modification of a collection.
   */
  fastify.patch("/collections/:id", { preHandler: [authenticate] }, async (request, reply) => {
    const userId = request.user.sub;
    const { id: colId } = request.params as { id: string };
    const parseResult = updateCollectionSchema.safeParse(request.body);
    if (!parseResult.success) {
      throw new ApiError(parseResult.error.issues[0].message, 400, "BAD_REQUEST");
    }
    const { name, color } = parseResult.data;

    try {
      const [colRecord] = await db
        .select()
        .from(collections)
        .where(and(eq(collections.id, colId), eq(collections.ownerId, userId)))
        .limit(1);

      if (!colRecord) {
        throw new ApiError("Collection not found.", 404, "COLLECTION_NOT_FOUND");
      }

      const updateData: any = {};
      if (name !== undefined) updateData.name = name;
      if (color !== undefined) updateData.color = color;

      const [updatedCol] = await db
        .update(collections)
        .set(updateData)
        .where(eq(collections.id, colId))
        .returning();

      reply.code(200).send({ data: updatedCol });
    } catch (err: any) {
      if (err instanceof ApiError) throw err;
      throw new ApiError(err.message || "Failed to update collection", 500, "INTERNAL_SERVER_ERROR");
    }
  });
}
