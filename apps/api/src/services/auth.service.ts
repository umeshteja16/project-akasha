import { db } from "../db.js";
import { users, sessions, eq } from "@akasha/db";
import bcrypt from "bcrypt";
import crypto from "crypto";

export interface UserResponse {
  id: string;
  email: string;
  createdAt: Date;
}

export interface AuthResult {
  user: {
    id: string;
    email: string;
  };
  refreshToken: string;
}

export class AuthService {
  /**
   * Hashes user password with bcrypt (cost 12) and inserts user into DB.
   */
  static async registerUser(email: string, password: string): Promise<UserResponse> {
    const sanitizedEmail = email.toLowerCase().trim();

    // Check if user already exists
    const [existingUser] = await db
      .select()
      .from(users)
      .where(eq(users.email, sanitizedEmail))
      .limit(1);

    if (existingUser) {
      const err = new Error("User with this email already exists");
      (err as any).statusCode = 409;
      throw err;
    }

    // Hash password with bcrypt cost factor 12
    const passwordHash = await bcrypt.hash(password, 12);

    const [newUser] = await db
      .insert(users)
      .values({
        email: sanitizedEmail,
        passwordHash,
      })
      .returning();

    return {
      id: newUser.id,
      email: newUser.email,
      createdAt: newUser.createdAt,
    };
  }

  /**
   * Validates credentials and creates a new DB-backed session with a random refresh token.
   */
  static async loginUser(
    email: string,
    password: string,
    userAgent?: string,
    ipAddress?: string
  ): Promise<AuthResult> {
    const sanitizedEmail = email.toLowerCase().trim();

    // Find user by email
    const [user] = await db
      .select()
      .from(users)
      .where(eq(users.email, sanitizedEmail))
      .limit(1);

    if (!user) {
      const err = new Error("Invalid email or password");
      (err as any).statusCode = 401;
      throw err;
    }

    // Verify password hash
    const isValid = await bcrypt.compare(password, user.passwordHash);
    if (!isValid) {
      const err = new Error("Invalid email or password");
      (err as any).statusCode = 401;
      throw err;
    }

    // Generate random refresh token (32 bytes)
    const refreshToken = crypto.randomBytes(32).toString("hex");

    // SHA-256 hash the refresh token before database storage
    const refreshTokenHash = crypto
      .createHash("sha256")
      .update(refreshToken)
      .digest("hex");

    // Session expiry: 7 days
    const expiresAt = new Date(Date.now() + 7 * 24 * 60 * 60 * 1000);

    // Insert session into DB
    await db.insert(sessions).values({
      userId: user.id,
      refreshTokenHash,
      expiresAt,
      userAgent,
      ipAddress,
    });

    return {
      user: {
        id: user.id,
        email: user.email,
      },
      refreshToken,
    };
  }

  /**
   * Rotates a refresh token, invalidating the old session and issuing a new one.
   */
  static async rotateSession(
    oldRefreshToken: string,
    userAgent?: string,
    ipAddress?: string
  ): Promise<AuthResult> {
    // Hash old refresh token to find it in the DB
    const oldHash = crypto
      .createHash("sha256")
      .update(oldRefreshToken)
      .digest("hex");

    // Find active session
    const [session] = await db
      .select()
      .from(sessions)
      .where(eq(sessions.refreshTokenHash, oldHash))
      .limit(1);

    if (!session) {
      const err = new Error("Invalid or expired session");
      (err as any).statusCode = 401;
      throw err;
    }

    // Check expiry
    if (session.expiresAt.getTime() < Date.now()) {
      // Clean up expired session
      await db.delete(sessions).where(eq(sessions.id, session.id));
      const err = new Error("Session has expired");
      (err as any).statusCode = 401;
      throw err;
    }

    // Fetch user details
    const [user] = await db
      .select()
      .from(users)
      .where(eq(users.id, session.userId))
      .limit(1);

    if (!user) {
      const err = new Error("User associated with session not found");
      (err as any).statusCode = 401;
      throw err;
    }

    // Generate new refresh token
    const newRefreshToken = crypto.randomBytes(32).toString("hex");
    const newHash = crypto
      .createHash("sha256")
      .update(newRefreshToken)
      .digest("hex");

    // Session expiry: 7 days
    const expiresAt = new Date(Date.now() + 7 * 24 * 60 * 60 * 1000);

    // Transaction to rotate: Delete old session and insert new session
    await db.transaction(async (tx) => {
      await tx.delete(sessions).where(eq(sessions.id, session.id));
      await tx.insert(sessions).values({
        userId: user.id,
        refreshTokenHash: newHash,
        expiresAt,
        userAgent: userAgent || session.userAgent,
        ipAddress: ipAddress || session.ipAddress,
      });
    });

    return {
      user: {
        id: user.id,
        email: user.email,
      },
      refreshToken: newRefreshToken,
    };
  }

  /**
   * Revokes an active session by refresh token hash (used for logout).
   */
  static async revokeSession(refreshToken: string): Promise<void> {
    const hash = crypto
      .createHash("sha256")
      .update(refreshToken)
      .digest("hex");

    await db.delete(sessions).where(eq(sessions.refreshTokenHash, hash));
  }
}
