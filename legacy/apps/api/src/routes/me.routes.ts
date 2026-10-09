import { FastifyInstance, FastifyPluginOptions } from "fastify";
import { authenticate } from "../middleware/auth.js";
import { ApiError } from "../errors/api-error.js";
import { db } from "../db.js";
import { auditLog, files, users, eq, sql } from "@akasha/db";
import { z } from "zod";

const updateMeSchema = z.object({
  displayName: z.string().min(1, "Display name cannot be empty").optional(),
});

export async function meRoutes(fastify: FastifyInstance, options: FastifyPluginOptions) {
  /**
   * GET /api/v1/me
   * Protected route returning authenticated user context.
   */
  fastify.get(
    "/me",
    { preHandler: [authenticate] },
    async (request, reply) => {
      const userId = request.user?.sub;
      if (!userId) {
        throw new ApiError("User context not found in request", 401, "UNAUTHORIZED");
      }

      try {
        // Fetch user from DB to get latest displayName
        const [userRecord] = await db
          .select({ email: users.email, displayName: users.displayName })
          .from(users)
          .where(eq(users.id, userId))
          .limit(1);

        // Structured Logging
        request.log.info(
          {
            action: "GET_PROFILE",
            userId,
            status: "success",
          },
          "User profile retrieved successfully"
        );

        reply.code(200).send({
          data: {
            id: userId,
            email: userRecord?.email || request.user.email,
            displayName: userRecord?.displayName || null,
          },
        });
      } catch (err: any) {
        throw new ApiError(err.message || "Failed to retrieve profile info", 500, "INTERNAL_SERVER_ERROR");
      }
    }
  );

  /**
   * PATCH /api/v1/me
   * Protected route to update profile details.
   */
  fastify.patch(
    "/me",
    { preHandler: [authenticate] },
    async (request, reply) => {
      const userId = request.user?.sub;
      if (!userId) {
        throw new ApiError("User context not found in request", 401, "UNAUTHORIZED");
      }

      const parseResult = updateMeSchema.safeParse(request.body);
      if (!parseResult.success) {
        throw new ApiError(parseResult.error.issues[0].message, 400, "BAD_REQUEST");
      }
      const { displayName } = parseResult.data;

      try {
        const updateData: any = { updatedAt: new Date() };
        if (displayName !== undefined) {
          updateData.displayName = displayName;
        }

        const [updatedUser] = await db
          .update(users)
          .set(updateData)
          .where(eq(users.id, userId))
          .returning();

        request.log.info(
          {
            action: "UPDATE_PROFILE",
            userId,
            displayName,
            status: "success",
          },
          "User profile updated successfully"
        );

        reply.code(200).send({
          data: {
            id: userId,
            email: updatedUser.email,
            displayName: updatedUser.displayName,
          },
        });
      } catch (err: any) {
        request.log.error(err, "Failed to update user profile");
        throw new ApiError(err.message || "Failed to update profile.", 500, "INTERNAL_SERVER_ERROR");
      }
    }
  );

  /**
   * GET /api/v1/activity
   * Protected owner-scoped list of chronological user activities.
   */
  fastify.get(
    "/activity",
    { preHandler: [authenticate] },
    async (request, reply) => {
      const userId = request.user?.sub;
      if (!userId) {
        throw new ApiError("User context not found in request", 401, "UNAUTHORIZED");
      }

      try {
        const list = await db
          .select({
            id: auditLog.id,
            action: auditLog.action,
            ipAddress: auditLog.ipAddress,
            createdAt: auditLog.createdAt,
            fileId: auditLog.fileId,
            fileName: files.originalName,
          })
          .from(auditLog)
          .leftJoin(files, eq(auditLog.fileId, files.id))
          .where(eq(auditLog.userId, userId))
          .orderBy(sql`${auditLog.createdAt} DESC`)
          .limit(100);

        reply.code(200).send({ data: list });
      } catch (err: any) {
        request.log.error(err, "Failed to retrieve user activities");
        throw new ApiError(err.message || "Failed to fetch activities.", 500, "INTERNAL_SERVER_ERROR");
      }
    }
  );
}
