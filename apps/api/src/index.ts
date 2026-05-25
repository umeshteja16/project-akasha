import Fastify from "fastify";
import cors from "@fastify/cors";
import cookie from "@fastify/cookie";
import jwt from "@fastify/jwt";
import multipart from "@fastify/multipart";
import rateLimit from "@fastify/rate-limit";
import * as dotenv from "dotenv";
import { validateEnv } from "@akasha/shared";
import { authRoutes } from "./routes/auth.routes.js";
import { meRoutes } from "./routes/me.routes.js";
import { filesRoutes } from "./routes/files.routes.js";
import { searchRoutes } from "./routes/search.routes.js";
import { chatRoutes } from "./routes/chat.routes.js";
import { userRoutes } from "./routes/user.routes.js";
import { ApiError } from "./errors/api-error.js";
import { db } from "./db.js";
import { sql } from "@akasha/db";

// Load environment variables locally
dotenv.config();

// Validate env at startup
const env = validateEnv(process.env);

// Physically check and migrate database schema to introduce missing columns on start
try {
  await db.execute(sql`ALTER TABLE files ADD COLUMN IF NOT EXISTS is_pinned boolean NOT NULL DEFAULT false;`);
  await db.execute(sql`ALTER TABLE users ADD COLUMN IF NOT EXISTS display_name text;`);
  console.log("Database schema initialized: is_pinned and display_name columns verified/added successfully.");
} catch (err) {
  console.error("Database schema migration on startup failed:", err);
}

// Scaffold Fastify server with Pino logger
const server = Fastify({
  logger: {
    level: env.NODE_ENV === "development" ? "debug" : "info",
    serializers: {
      req(request) {
        return {
          method: request.method,
          url: request.url,
          hostname: request.hostname,
          remoteAddress: request.ip,
        };
      },
    },
  },
});

// Setup plugins
await server.register(cors, {
  origin: true, // In development, allow all origins
  credentials: true,
});

await server.register(cookie);

await server.register(jwt, {
  secret: env.JWT_SECRET,
  cookie: {
    cookieName: "refreshToken",
    signed: false,
  },
});

await server.register(multipart, {
  limits: {
    fileSize: env.MAX_FILE_SIZE_MB * 1024 * 1024,
  },
});

// Global Rate Limiting: 100 requests/min in production, scaled up in development to allow intensive loopback stress testing
await server.register(rateLimit, {
  global: true,
  max: env.NODE_ENV === "development" ? 10000 : 100,
  timeWindow: "1 minute",
});

// Centralized Global Error Handler
server.setErrorHandler((error: any, request, reply) => {
  // 1. Rate Limit Exceeded
  if (error.statusCode === 429) {
    request.log.warn(
      {
        action: "RATE_LIMIT_HIT",
        ip: request.ip,
        url: request.url,
        status: "blocked",
      },
      "Rate limit exceeded"
    );

    reply.status(429).send({
      error: {
        code: "RATE_LIMIT_EXCEEDED",
        message: "Too many requests. Please slow down and try again later.",
      },
    });
    return;
  }

  // 2. Custom Typed API Errors
  if (error instanceof ApiError) {
    request.log.warn(
      {
        action: "API_ERROR",
        statusCode: error.statusCode,
        code: error.code,
        message: error.message,
      },
      `API error thrown: ${error.message}`
    );

    reply.status(error.statusCode).send({
      error: {
        code: error.code,
        message: error.message,
      },
    });
    return;
  }

  // 3. Fastify Validation Errors
  if (error.validation) {
    reply.status(400).send({
      error: {
        code: "VALIDATION_ERROR",
        message: error.message,
      },
    });
    return;
  }

  // 4. Default Internal/Server Errors
  request.log.error(error, "Unexpected server error caught in global handler");
  const statusCode = error.statusCode || 500;
  reply.status(statusCode).send({
    error: {
      code: "INTERNAL_SERVER_ERROR",
      message: "An unexpected error occurred on the server.",
    },
  });
});

// Register Authentication and Files Routes
await server.register(authRoutes, { prefix: "/api/v1/auth" });
await server.register(meRoutes, { prefix: "/api/v1" });
await server.register(filesRoutes, { prefix: "/api/v1" });
await server.register(searchRoutes, { prefix: "/api/v1" });
await server.register(chatRoutes, { prefix: "/api/v1" });
await server.register(userRoutes, { prefix: "/api/v1/user" });

// Standard Healthcheck Route
server.get("/api/v1/health", async () => {
  return { 
    status: "ok", 
    timestamp: new Date().toISOString(),
    strictOffline: env.STRICT_OFFLINE 
  };
});

// Start the server
const start = async () => {
  try {
    await server.listen({ port: env.PORT, host: "0.0.0.0" });
    server.log.info(`🚀 AKASHA API server running on port ${env.PORT}`);
  } catch (err) {
    server.log.error(err);
    process.exit(1);
  }
};

start();
