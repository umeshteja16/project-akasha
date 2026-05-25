import { FastifyInstance, FastifyPluginOptions } from "fastify";
import { authenticate } from "../middleware/auth.js";
import { ApiError } from "../errors/api-error.js";
import { users, files, fileChunks, eq, and, sql } from "@akasha/db";
import { db } from "../db.js";
import { StorageService } from "../services/storage.service.js";
import bcrypt from "bcrypt";
import { logActivity } from "../services/audit.service.js";
import { exec } from "node:child_process";
import { promisify } from "node:util";
import { z } from "zod";

const execAsync = promisify(exec);

const changePasswordBodySchema = z.object({
  currentPassword: z.string().min(1, "Current password is required"),
  newPassword: z.string().min(8, "New password must be at least 8 characters long"),
});

async function getDiskFreeSpace(): Promise<number> {
  try {
    const dir = process.env.UPLOAD_DIR || "./storage";
    const { stdout } = await execAsync(`df -k "${dir}"`);
    const lines = stdout.trim().split("\n");
    if (lines.length >= 2) {
      const parts = lines[1].split(/\s+/);
      const availableKB = parseInt(parts[3], 10);
      if (!isNaN(availableKB)) {
        return availableKB * 1024;
      }
    }
  } catch (err) {
    // Fallback: 10GB
    return 10 * 1024 * 1024 * 1024;
  }
  return 10 * 1024 * 1024 * 1024;
}

export async function userRoutes(fastify: FastifyInstance, options: FastifyPluginOptions) {
  /**
   * GET /api/v1/user/usage
   * Protected endpoint retrieving current storage usage, file totals, and chunk counts.
   */
  fastify.get(
    "/usage",
    { preHandler: [authenticate] },
    async (request, reply) => {
      const userId = request.user?.sub;
      if (!userId) {
        throw new ApiError("User context not found in request", 401, "UNAUTHORIZED");
      }

      try {
        // 1. Fetch user creation date
        const [userRecord] = await db
          .select({ createdAt: users.createdAt, storageLimit: users.storageLimit })
          .from(users)
          .where(eq(users.id, userId))
          .limit(1);
 
        if (!userRecord) {
          throw new ApiError("User not found", 404, "USER_NOT_FOUND");
        }
 
        // 2. Fetch total files count and total storage bytes used
        const [filesUsage] = await db
          .select({
            count: sql<number>`count(${files.id})::int`,
            totalBytes: sql<number>`coalesce(sum(${files.sizeBytes}), 0)::int`,
          })
          .from(files)
          .where(eq(files.ownerId, userId));
 
        // 3. Fetch total indexed chunks count
        const [chunksUsage] = await db
          .select({
            count: sql<number>`count(${fileChunks.id})::int`,
          })
          .from(fileChunks)
          .innerJoin(files, eq(fileChunks.fileId, files.id))
          .where(eq(files.ownerId, userId));
 
        const diskFree = await getDiskFreeSpace();
 
        reply.code(200).send({
          data: {
            userCreatedAt: userRecord.createdAt.toISOString(),
            fileCount: filesUsage?.count || 0,
            storageUsed: filesUsage?.totalBytes || 0,
            storageLimit: userRecord.storageLimit,
            chunkCount: chunksUsage?.count || 0,
            diskFreeSpace: diskFree,
          },
        });
      } catch (err: any) {
        if (err instanceof ApiError) throw err;
        request.log.error(err, "Failed to retrieve user usage stats");
        throw new ApiError(err.message || "Failed to fetch user usage stats.", 500, "INTERNAL_SERVER_ERROR");
      }
    }
  );
 
  /**
   * PATCH /api/v1/user/storage-limit
   * Protected endpoint to update the current user's storage limit.
   */
  fastify.patch(
    "/storage-limit",
    { preHandler: [authenticate] },
    async (request, reply) => {
      const userId = request.user?.sub;
      if (!userId) {
        throw new ApiError("User context not found in request", 401, "UNAUTHORIZED");
      }
 
      const { limitMB } = request.body as { limitMB?: number };
      if (limitMB === undefined || typeof limitMB !== "number" || limitMB <= 0) {
        throw new ApiError("Invalid or missing storage limit value.", 400, "BAD_REQUEST");
      }
 
      try {
        const limitBytes = limitMB * 1024 * 1024;
 
        await db
          .update(users)
          .set({
            storageLimit: limitBytes,
            updatedAt: new Date(),
          })
          .where(eq(users.id, userId));
 
        request.log.info(
          { action: "SET_STORAGE_LIMIT", userId, limitMB, status: "success" },
          "Storage limit set successfully"
        );
 
        reply.code(200).send({
          success: true,
          message: `Storage limit set to ${limitMB}MB.`,
          data: {
            storageLimit: limitBytes,
          },
        });
      } catch (err: any) {
        request.log.error(err, "Failed to update storage limit");
        throw new ApiError(err.message || "Failed to update storage limit.", 500, "INTERNAL_SERVER_ERROR");
      }
    }
  );

  /**
   * POST /api/v1/user/change-password
   * Protected endpoint to safely update current user's password.
   */
  fastify.post(
    "/change-password",
    { preHandler: [authenticate] },
    async (request, reply) => {
      const userId = request.user?.sub;
      if (!userId) {
        throw new ApiError("User context not found in request", 401, "UNAUTHORIZED");
      }

      const parseResult = changePasswordBodySchema.safeParse(request.body);
      if (!parseResult.success) {
        throw new ApiError(parseResult.error.issues[0].message, 400, "BAD_REQUEST");
      }

      const { currentPassword, newPassword } = parseResult.data;

      try {
        // Fetch user from DB
        const [userRecord] = await db
          .select()
          .from(users)
          .where(eq(users.id, userId))
          .limit(1);

        if (!userRecord) {
          throw new ApiError("User not found.", 404, "USER_NOT_FOUND");
        }

        // Verify current password hash
        const isCurrentValid = await bcrypt.compare(currentPassword, userRecord.passwordHash);
        if (!isCurrentValid) {
          throw new ApiError("Current password hash mismatch.", 400, "PASSWORD_MISMATCH");
        }

        // Hash new password using bcrypt factor 12
        const newPasswordHash = await bcrypt.hash(newPassword, 12);

        // Update in database
        await db
          .update(users)
          .set({
            passwordHash: newPasswordHash,
            updatedAt: new Date(),
          })
          .where(eq(users.id, userId));

        request.log.info({ action: "CHANGE_PASSWORD", userId, status: "success" }, "Password changed successfully");

        await logActivity({
          userId,
          action: "CHANGE_PASSWORD",
          ipAddress: request.ip,
        });

        reply.code(200).send({ success: true, message: "Password updated successfully." });
      } catch (err: any) {
        if (err instanceof ApiError) throw err;
        request.log.error(err, "Failed to change user password");
        throw new ApiError(err.message || "Failed to update password.", 500, "INTERNAL_SERVER_ERROR");
      }
    }
  );

  /**
   * DELETE /api/v1/user/account
   * Protected endpoint for irreversible cascade deletion of the current user's account and data.
   */
  fastify.delete(
    "/account",
    { preHandler: [authenticate] },
    async (request, reply) => {
      const userId = request.user?.sub;
      if (!userId) {
        throw new ApiError("User context not found in request", 401, "UNAUTHORIZED");
      }

      try {
        // 1. Fetch all file storage paths owned by the user to perform physical disk cleanups
        const userFiles = await db
          .select({ storagePath: files.storagePath })
          .from(files)
          .where(eq(files.ownerId, userId));

        // 2. Perform cascade database deletion of the user record in PostgreSQL (Drizzle foreign keys are cascade-enabled)
        await db.delete(users).where(eq(users.id, userId));

        // 3. Clean up all physical disk storage assets safely
        for (const fileRecord of userFiles) {
          try {
            // Safe de-duplication check: delete only if no remaining references in files table (though the cascade deleted this user's files, it's good practice)
            const [stillExists] = await db
              .select()
              .from(files)
              .where(eq(files.storagePath, fileRecord.storagePath))
              .limit(1);

            if (!stillExists) {
              await StorageService.deleteFile(fileRecord.storagePath);
            }
          } catch (err) {
            request.log.error(err, `Failed to clean up physical storage for path: ${fileRecord.storagePath}`);
          }
        }

        // 4. Revoke cookie session if active
        reply.clearCookie("refreshToken", { path: "/api/v1/auth" });

        request.log.info({ action: "DELETE_ACCOUNT", userId, status: "success" }, "User account cascade deleted successfully");

        reply.code(200).send({ success: true, message: "Account successfully deleted." });
      } catch (err: any) {
        request.log.error(err, "Failed to delete user account");
        throw new ApiError(err.message || "Failed to delete account.", 500, "INTERNAL_SERVER_ERROR");
      }
    }
  );
}
