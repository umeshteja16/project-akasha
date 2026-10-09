import { z } from "zod";

// Sane filename validation: no path traversals, alphanumeric with reasonable punctuation, max 255 chars
export const filenameSchema = z.string()
  .min(1, "Filename is required")
  .max(255, "Filename is too long (max 255 characters)")
  .regex(/^[^\/\\0]+$/, "Filename contains invalid characters")
  .refine((val) => !val.includes("..") && !val.includes("/") && !val.includes("\\"), {
    message: "Filename contains path traversal sequences",
  });

export const SUPPORTED_MIME_TYPES = [
  "application/pdf",
  "image/jpeg",
  "image/png",
  "image/webp",
  "text/plain",
  "text/markdown",
  "text/x-markdown",
  "text/csv",
  "video/mp4",
  "video/webm",
  "video/x-matroska",
  "audio/mpeg",
  "audio/wav"
] as const;

export type SupportedMimeType = typeof SUPPORTED_MIME_TYPES[number];

export function isSupportedMimeType(mime: string): mime is SupportedMimeType {
  return SUPPORTED_MIME_TYPES.includes(mime as any);
}

export const mimeTypeSchema = z.string();

export const fileUploadSchema = z.object({
  filename: filenameSchema,
  mimeType: mimeTypeSchema,
  // 5GB maximum size limit (allows huge uploads)
  sizeBytes: z.number().max(5 * 1024 * 1024 * 1024, "File exceeds the maximum size limit of 5GB"),
});


export type FileUploadInput = z.infer<typeof fileUploadSchema>;

/**
 * Validates the magic bytes (file signature) of an uploaded file against its declared MIME type.
 * Takes the first 12-16 bytes of the file.
 * Returns false if the magic bytes strongly mismatch the declared type (indicating a masqueraded binary).
 */
export function verifyFileMagicBytes(header: Uint8Array, mimeType: string): boolean {
  const hex = Array.from(header)
    .map((b) => b.toString(16).padStart(2, "0").toLowerCase())
    .join("");

  switch (mimeType) {
    case "application/pdf":
      // PDF magic bytes: %PDF (25 50 44 46)
      return hex.startsWith("25504446");

    case "image/png":
      // PNG magic bytes: 89 50 4e 47 0d 0a 1a 0a
      return hex.startsWith("89504e470d0a1a0a");

    case "image/jpeg":
      // JPEG magic bytes: ff d8 ff
      return hex.startsWith("ffd8ff");

    case "image/webp":
      // WEBP magic bytes: RIFF at index 0 (52 49 46 46) and WEBP at index 8 (57 45 42 50)
      return hex.startsWith("52494646") && hex.substring(16, 24) === "57454250";

    case "video/mp4":
      // MP4 magic bytes: ftyp at index 4 (66 74 79 70)
      return hex.substring(8, 16) === "66747970";

    case "video/webm":
    case "video/x-matroska":
      // EBML header used by WebM & MKV: 1a 45 df a3
      return hex.startsWith("1a45dfa3");

    case "audio/mpeg":
      // MP3 magic bytes: ID3 (49 44 33) or frame sync (ff fb / ff f3 / ff f2)
      return (
        hex.startsWith("494433") ||
        hex.startsWith("fffb") ||
        hex.startsWith("fff3") ||
        hex.startsWith("fff2")
      );

    case "audio/wav":
      // WAV magic bytes: RIFF at index 0 (52 49 46 46) and WAVE at index 8 (57 41 56 45)
      return hex.startsWith("52494646") && hex.substring(16, 24) === "57415645";

    case "text/plain":
    case "text/markdown":
    case "text/x-markdown":
    case "text/csv": {
      // Plaintext files shouldn't contain binary null bytes (00) or control characters
      for (let i = 0; i < header.length; i++) {
        const b = header[i];
        if (b === 0) return false; // Contains NULL byte -> binary payload masquerading as text
      }
      return true;
    }

    default:
      // Unknown or unvalidated types pass by default
      return true;
  }
}
