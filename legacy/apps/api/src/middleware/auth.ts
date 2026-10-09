import { FastifyReply, FastifyRequest } from "fastify";
import "@fastify/jwt";
import { ApiError } from "../errors/api-error.js";

// Extend Fastify's types to include custom JWT payload and request.user
declare module "@fastify/jwt" {
  interface FastifyJWT {
    payload: {
      sub: string;
      email: string;
    };
    user: {
      sub: string;
      email: string;
    };
  }
}

/**
 * Fastify preHandler middleware that extracts the Bearer JWT from headers,
 * verifies it, and attaches the decoded user context to request.user.
 */
export async function authenticate(request: FastifyRequest, reply: FastifyReply) {
  try {
    await request.jwtVerify();
  } catch (err: any) {
    throw new ApiError(err.message || "Invalid or missing token", 401, "UNAUTHORIZED");
  }
}

