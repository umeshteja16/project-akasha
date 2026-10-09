import fs from "fs";
import path from "path";
import crypto from "crypto";
import { Readable } from "stream";

export interface UploadResult {
  contentHash: string;
  storagePath: string;
  sizeBytes: number;
}

export class StorageService {
  /**
   * Streams a file upload to a temporary path, calculates SHA-256 hash on-the-fly,
   * counts bytes, and moves the completed file to a content-addressed path.
   * If a file with the same hash already exists, the temp file is unlinked and
   * the existing storage path is reused (automatic storage de-duplication).
   */
  static async saveStream(
    fileStream: Readable,
    uploadDir: string
  ): Promise<UploadResult> {
    // 1. Resolve and ensure upload directory exists
    const resolvedUploadDir = path.resolve(uploadDir);
    await fs.promises.mkdir(resolvedUploadDir, { recursive: true });

    // 2. Stream to a temporary file while calculating hash and size
    const tempFileName = `tmp_${crypto.randomUUID()}`;
    const tempFilePath = path.join(resolvedUploadDir, tempFileName);

    const writeStream = fs.createWriteStream(tempFilePath);
    const hash = crypto.createHash("sha256");
    let sizeBytes = 0;

    try {
      await new Promise<void>((resolve, reject) => {
        fileStream.on("data", (chunk: Buffer) => {
          sizeBytes += chunk.length;
          hash.update(chunk);

          if (!writeStream.write(chunk)) {
            fileStream.pause();
            writeStream.once("drain", () => fileStream.resume());
          }
        });

        fileStream.on("end", () => {
          writeStream.end();
        });

        fileStream.on("error", (err) => {
          writeStream.destroy();
          reject(err);
        });

        writeStream.on("finish", () => {
          resolve();
        });

        writeStream.on("error", (err) => {
          reject(err);
        });
      });
    } catch (err) {
      // In case of stream errors, attempt to delete the temporary file if it was created
      try {
        await fs.promises.unlink(tempFilePath);
      } catch {
        // Ignore deletion errors for temp file
      }
      throw err;
    }

    // 3. Complete hash computation
    const contentHash = hash.digest("hex");
    const finalFilePath = path.join(resolvedUploadDir, contentHash);

    // 4. Content-addressed storage move with de-duplication check
    try {
      // Check if file with same hash already exists
      await fs.promises.access(finalFilePath);
      // It exists! Delete the temporary file and reuse the existing one
      await fs.promises.unlink(tempFilePath);
    } catch {
      // File does not exist yet. Move temp file to its final content-addressed path
      await fs.promises.rename(tempFilePath, finalFilePath);
    }

    return {
      contentHash,
      storagePath: finalFilePath,
      sizeBytes,
    };
  }

  /**
   * Safely deletes a file from the disk.
   */
  static async deleteFile(storagePath: string): Promise<void> {
    try {
      await fs.promises.unlink(storagePath);
    } catch (err: any) {
      // Ignore if file was already deleted/not found
      if (err.code !== "ENOENT") {
        throw err;
      }
    }
  }
}
