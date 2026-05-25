import { FastifyInstance, FastifyPluginOptions } from "fastify";
import { registerSchema, loginSchema } from "@akasha/shared";
import { AuthService } from "../services/auth.service.js";
import { ApiError } from "../errors/api-error.js";
import { logActivity } from "../services/audit.service.js";

export async function authRoutes(fastify: FastifyInstance, options: FastifyPluginOptions) {
  /**
   * POST /api/v1/auth/register
   * Registers a new user.
   * Rate limited: 10 attempts per minute.
   */
  fastify.post(
    "/register",
    {
      config: {
        rateLimit: {
          max: process.env.NODE_ENV === "development" ? 10000 : 10,
          timeWindow: "1 minute",
        },
      },
    },
    async (request, reply) => {
      const parseResult = registerSchema.safeParse(request.body);
      if (!parseResult.success) {
        throw new ApiError(parseResult.error.issues[0].message, 400, "BAD_REQUEST");
      }

      const { email, password } = parseResult.data;

      try {
        const user = await AuthService.registerUser(email, password);

        // Structured Logging
        request.log.info(
          {
            action: "REGISTER",
            userId: user.id,
            status: "success",
          },
          "User registered successfully"
        );

        // Persistent Audit Logging
        await logActivity({
          userId: user.id,
          action: "REGISTER",
          ipAddress: request.ip,
        });

        reply.code(201).send({ user });
      } catch (err: any) {
        const statusCode = err.statusCode || 500;
        const code = statusCode === 409 ? "CONFLICT" : "INTERNAL_SERVER_ERROR";
        throw new ApiError(err.message, statusCode, code);
      }
    }
  );

  /**
   * POST /api/v1/auth/login
   * Authenticates user, stores rotating session in DB, returns minimal JWT.
   * Rate limited: 10 attempts per minute.
   */
  fastify.post(
    "/login",
    {
      config: {
        rateLimit: {
          max: process.env.NODE_ENV === "development" ? 10000 : 10,
          timeWindow: "1 minute",
        },
      },
    },
    async (request, reply) => {
      const parseResult = loginSchema.safeParse(request.body);
      if (!parseResult.success) {
        throw new ApiError(parseResult.error.issues[0].message, 400, "BAD_REQUEST");
      }

      const { email, password } = parseResult.data;
      const userAgent = request.headers["user-agent"];
      const ipAddress = request.ip;

      try {
        const result = await AuthService.loginUser(email, password, userAgent, ipAddress);

        // Sign minimal JWT access token (15 mins expiry)
        const accessToken = fastify.jwt.sign(
          {
            sub: result.user.id,
            email: result.user.email,
          },
          { expiresIn: "15m" }
        );

        // Set rotating refresh token in cookie
        reply.setCookie("refreshToken", result.refreshToken, {
          path: "/api/v1/auth",
          httpOnly: true,
          secure: process.env.NODE_ENV === "production",
          sameSite: "lax",
          maxAge: 7 * 24 * 60 * 60, // 7 days in seconds
        });

        // Structured Logging
        request.log.info(
          {
            action: "LOGIN",
            userId: result.user.id,
            status: "success",
          },
          "User logged in successfully"
        );

        // Persistent Audit Logging
        await logActivity({
          userId: result.user.id,
          action: "LOGIN",
          ipAddress,
        });

        reply.code(200).send({
          accessToken,
          user: result.user,
        });
      } catch (err: any) {
        const statusCode = err.statusCode || 500;
        const code = statusCode === 401 ? "UNAUTHORIZED" : "INTERNAL_SERVER_ERROR";
        throw new ApiError(err.message, statusCode, code);
      }
    }
  );

  /**
   * POST /api/v1/auth/refresh
   * Rotates database refresh session, issues a new JWT and refreshed cookie.
   * Rate limited: 10 attempts per minute.
   */
  fastify.post(
    "/refresh",
    {
      config: {
        rateLimit: {
          max: process.env.NODE_ENV === "development" ? 10000 : 10,
          timeWindow: "1 minute",
        },
      },
    },
    async (request, reply) => {
      const oldRefreshToken = request.cookies.refreshToken;
      if (!oldRefreshToken) {
        throw new ApiError("Refresh token is missing", 401, "UNAUTHORIZED");
      }

      const userAgent = request.headers["user-agent"];
      const ipAddress = request.ip;

      try {
        const result = await AuthService.rotateSession(oldRefreshToken, userAgent, ipAddress);

        // Sign new minimal JWT
        const accessToken = fastify.jwt.sign(
          {
            sub: result.user.id,
            email: result.user.email,
          },
          { expiresIn: "15m" }
        );

        // Set new rotated refresh token cookie
        reply.setCookie("refreshToken", result.refreshToken, {
          path: "/api/v1/auth",
          httpOnly: true,
          secure: process.env.NODE_ENV === "production",
          sameSite: "lax",
          maxAge: 7 * 24 * 60 * 60, // 7 days in seconds
        });

        // Structured Logging
        request.log.info(
          {
            action: "REFRESH",
            userId: result.user.id,
            status: "success",
          },
          "User session refreshed successfully"
        );

        reply.code(200).send({
          accessToken,
          user: result.user,
        });
      } catch (err: any) {
        // Clear refresh cookie on failed validation
        reply.clearCookie("refreshToken", {
          path: "/api/v1/auth",
        });

        throw new ApiError(err.message || "Invalid or expired session", 401, "UNAUTHORIZED");
      }
    }
  );

  /**
   * POST /api/v1/auth/logout
   * Revokes the session from the DB and clears the HTTP-only cookie.
   */
  fastify.post("/logout", async (request, reply) => {
    const refreshToken = request.cookies.refreshToken;

    try {
      if (refreshToken) {
        await AuthService.revokeSession(refreshToken);
      }

      // Structured Logging
      request.log.info(
        {
          action: "LOGOUT",
          status: "success",
        },
        "User logged out successfully"
      );
    } catch (err) {
      request.log.error(err, "Error revoking session during logout");
    } finally {
      // Always clear the cookie
      reply.clearCookie("refreshToken", {
        path: "/api/v1/auth",
      });

      reply.code(200).send({ success: true });
    }
  });
}
