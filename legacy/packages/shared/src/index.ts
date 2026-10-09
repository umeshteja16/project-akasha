export * from "./env.js";
export * from "./auth.js";
export * from "./file.js";
export interface FileMetadata {
  id: string;
  originalName: string;
  mimeType: string;
  sizeBytes: number;
  status: "pending" | "processing" | "completed" | "failed";
  createdAt: string;
}

