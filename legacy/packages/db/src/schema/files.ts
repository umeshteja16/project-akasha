import { pgTable, uuid, text, timestamp, integer, jsonb, customType, index, boolean } from "drizzle-orm/pg-core";
import { relations, sql } from "drizzle-orm";
import { users } from "./users.js";
import { auditLog } from "./audit-log.js";

// Custom PostgreSQL tsvector type mapping for full-text search indexing
const tsvector = customType<{ data: string }>({
  dataType() {
    return "tsvector";
  },
});

// Custom PostgreSQL vector type mapping for pgvector sentence embeddings
export const vector = customType<{ data: number[], config: { dimensions: number } }>({
  dataType(config) {
    return `vector(${config?.dimensions})`;
  },
  toDriver(value: number[]) {
    return `[${value.join(",")}]`;
  },
  fromDriver(value: unknown) {
    if (typeof value === "string") {
      return value.slice(1, -1).split(",").map(Number);
    }
    return value as number[];
  },
});

export const files = pgTable("files", {
  id: uuid("id").defaultRandom().primaryKey(),
  ownerId: uuid("owner_id")
    .notNull()
    .references(() => users.id, { onDelete: "cascade" }),
  originalName: text("original_name").notNull(),
  storagePath: text("storage_path").notNull(),
  contentHash: text("content_hash").notNull(),
  mimeType: text("mime_type").notNull(),
  sizeBytes: integer("size_bytes").notNull(),
  status: text("status", { enum: ["pending", "processing", "completed", "failed"] })
    .default("pending")
    .notNull(),
  tags: text("tags").array(),
  summary: text("summary"),
  isStarred: boolean("is_starred").default(false).notNull(),
  isPinned: boolean("is_pinned").default(false).notNull(),
  collectionId: uuid("collection_id").references(() => collections.id, { onDelete: "set null" }),
  archivedAt: timestamp("archived_at"),
  lastOpenedAt: timestamp("last_opened_at"),
  openCount: integer("open_count").default(0).notNull(),
  createdAt: timestamp("created_at").defaultNow().notNull(),
  updatedAt: timestamp("updated_at").defaultNow().notNull(),
});

export const fileExtractions = pgTable("file_extractions", {
  id: uuid("id").defaultRandom().primaryKey(),
  fileId: uuid("file_id")
    .notNull()
    .references(() => files.id, { onDelete: "cascade" }),
  extractedText: text("extracted_text").notNull(),
  meta: jsonb("meta"), // Stores page counts, processing durations, or engine-specific metadata
  createdAt: timestamp("created_at").defaultNow().notNull(),
  updatedAt: timestamp("updated_at").defaultNow().notNull(),
});

export const fileChunks = pgTable(
  "file_chunks",
  {
    id: uuid("id").defaultRandom().primaryKey(),
    fileId: uuid("file_id")
      .notNull()
      .references(() => files.id, { onDelete: "cascade" }),
    chunkIndex: integer("chunk_index").notNull(),
    chunkText: text("chunk_text").notNull(),
    tsvectorContent: tsvector("tsvector_content").notNull(),
    embedding: vector("embedding", { dimensions: 768 }),
    embeddingModel: text("embedding_model"),
    embeddingDimensions: integer("embedding_dimensions"),
    embeddingChecksum: text("embedding_checksum"),
    embeddedAt: timestamp("embedded_at"),
    createdAt: timestamp("created_at").defaultNow().notNull(),
  },
  (table) => [
    index("file_chunks_file_id_idx").on(table.fileId),
    index("file_chunks_tsvector_idx").using("gin", table.tsvectorContent),
    index("file_chunks_embedding_hnsw_idx").using("hnsw", sql`embedding vector_cosine_ops WITH (m = 32, ef_construction = 200)`),
  ]
);

export const filesRelations = relations(files, ({ one, many }) => ({
  owner: one(users, {
    fields: [files.ownerId],
    references: [users.id],
  }),
  auditLogs: many(auditLog),
  extraction: one(fileExtractions, {
    fields: [files.id],
    references: [fileExtractions.fileId],
  }),
  chunks: many(fileChunks),
}));

export const fileExtractionsRelations = relations(fileExtractions, ({ one }) => ({
  file: one(files, {
    fields: [fileExtractions.fileId],
    references: [files.id],
  }),
}));

export const fileChunksRelations = relations(fileChunks, ({ one }) => ({
  file: one(files, {
    fields: [fileChunks.fileId],
    references: [files.id],
  }),
}));

export const collections = pgTable("collections", {
  id: uuid("id").defaultRandom().primaryKey(),
  ownerId: uuid("owner_id")
    .notNull()
    .references(() => users.id, { onDelete: "cascade" }),
  name: text("name").notNull(),
  color: text("color").default("#6c63ff").notNull(),
  createdAt: timestamp("created_at").defaultNow().notNull(),
});

export const collectionsRelations = relations(collections, ({ one }) => ({
  owner: one(users, {
    fields: [collections.ownerId],
    references: [users.id],
  }),
}));

export const userActivity = pgTable("user_activity", {
  id: uuid("id").defaultRandom().primaryKey(),
  userId: uuid("user_id")
    .notNull()
    .references(() => users.id, { onDelete: "cascade" }),
  fileId: uuid("file_id").references(() => files.id, { onDelete: "set null" }),
  action: text("action", { enum: ["upload", "open", "search", "delete"] }).notNull(),
  metadata: jsonb("metadata"),
  createdAt: timestamp("created_at").defaultNow().notNull(),
});

export const userActivityRelations = relations(userActivity, ({ one }) => ({
  user: one(users, {
    fields: [userActivity.userId],
    references: [users.id],
  }),
  file: one(files, {
    fields: [userActivity.fileId],
    references: [files.id],
  }),
}));
