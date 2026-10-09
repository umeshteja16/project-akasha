import { pgTable, uuid, text, timestamp, bigint } from "drizzle-orm/pg-core";
import { relations, sql } from "drizzle-orm";
import { files } from "./files.js";
import { auditLog } from "./audit-log.js";
import { sessions } from "./sessions.js";

export const users = pgTable("users", {
  id: uuid("id").defaultRandom().primaryKey(),
  email: text("email").notNull().unique(),
  passwordHash: text("password_hash").notNull(),
  displayName: text("display_name"),
  storageLimit: bigint("storage_limit", { mode: "number" }).default(sql`52428800`).notNull(),
  createdAt: timestamp("created_at").defaultNow().notNull(),
  updatedAt: timestamp("updated_at").defaultNow().notNull(),
});

export const usersRelations = relations(users, ({ many }) => ({
  files: many(files),
  auditLogs: many(auditLog),
  sessions: many(sessions),
}));
