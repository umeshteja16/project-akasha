import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/client";
import type { FileItem } from "@/api/files";
import { overallProgress, UploadQueue } from "./upload-queue";
import { UploadAborted, type UploadOptions, type UploadResult } from "./xhr-upload";

function fileItem(id: string, name: string): FileItem {
  return {
    id,
    name,
    mime_type: "text/plain",
    size_bytes: 10,
    content_hash: "ab".repeat(32),
    status: "pending",
    is_pinned: false,
    tags: [],
    auto_tags: [],
    created_at: "2026-10-09T10:00:00Z",
    updated_at: "2026-10-09T10:00:00Z",
  };
}

/** An uploader whose calls the test settles by hand. */
function manualUploader() {
  const calls: Array<{
    file: File;
    options: UploadOptions;
    resolve: (r: UploadResult) => void;
    reject: (e: unknown) => void;
  }> = [];
  const upload = (file: File, options: UploadOptions) =>
    new Promise<UploadResult>((resolve, reject) => {
      options.signal.addEventListener("abort", () => reject(new UploadAborted()));
      calls.push({ file, options, resolve, reject });
    });
  return { upload, calls };
}

const make = (name: string, size = 10) =>
  new File(["x".repeat(size)], name, { type: "text/plain" });
const flush = () => new Promise((r) => setTimeout(r, 0));

describe("UploadQueue", () => {
  it("runs at most `concurrency` uploads and starts the next when one ends", async () => {
    const { upload, calls } = manualUploader();
    const queue = new UploadQueue({ upload, concurrency: 2 });
    queue.add([make("a.txt"), make("b.txt"), make("c.txt")]);
    expect(calls).toHaveLength(2);
    expect(queue.getSnapshot().map((i) => i.state)).toEqual(["uploading", "uploading", "queued"]);

    calls[0]?.resolve({ created: true, file: fileItem("1", "a.txt") });
    await flush();
    expect(calls).toHaveLength(3);
    expect(queue.getSnapshot()[0]?.state).toBe("done");
  });

  it("tracks progress and reports new files and duplicates", async () => {
    const { upload, calls } = manualUploader();
    const onUploaded = vi.fn();
    const queue = new UploadQueue({ upload, onUploaded });
    const listener = vi.fn();
    queue.subscribe(listener);
    queue.add([make("a.txt", 100), make("b.txt", 100)]);

    calls[0]?.options.onProgress(50, 100);
    expect(queue.getSnapshot()[0]?.loaded).toBe(50);
    expect(overallProgress(queue.getSnapshot())).toBeCloseTo(0.25);
    expect(listener).toHaveBeenCalled();

    calls[0]?.resolve({ created: true, file: fileItem("1", "a.txt") });
    calls[1]?.resolve({ created: false, file: fileItem("2", "b.txt") });
    await flush();
    const [a, b] = queue.getSnapshot();
    expect(a?.state).toBe("done");
    expect(b?.state).toBe("duplicate");
    expect(b?.file?.id).toBe("2");
    expect(onUploaded).toHaveBeenCalledTimes(2);
    expect(overallProgress(queue.getSnapshot())).toBe(1);
  });

  it("cancels an upload and lets it be retried", async () => {
    const { upload, calls } = manualUploader();
    const queue = new UploadQueue({ upload });
    const [id = ""] = queue.add([make("a.txt")]);
    queue.cancel(id);
    await flush();
    expect(calls[0]?.options.signal.aborted).toBe(true);
    expect(queue.getSnapshot()[0]?.state).toBe("cancelled");

    queue.retry(id);
    expect(calls).toHaveLength(2);
    expect(queue.getSnapshot()[0]?.state).toBe("uploading");
  });

  it("maps server errors and marks only transient ones retryable", async () => {
    const { upload, calls } = manualUploader();
    const queue = new UploadQueue({ upload, concurrency: 3 });
    queue.add([make("a.zip"), make("b.txt"), make("c.txt")]);
    calls[0]?.reject(new ApiError(415, "unsupported_media_type", "unsupported file type"));
    calls[1]?.reject(new ApiError(413, "quota_exceeded", "storage quota exceeded"));
    calls[2]?.reject(new ApiError(0, "network", "offline"));
    await flush();
    const [a, b, c] = queue.getSnapshot();
    expect(a?.state).toBe("failed");
    expect(a?.problem?.title).toBe("Can't keep this type");
    expect(a?.problem?.retryable).toBe(false);
    expect(b?.problem?.title).toBe("Library is full");
    expect(c?.problem?.retryable).toBe(true);
  });

  it("refuses files over the server's limit without sending them", () => {
    const { upload, calls } = manualUploader();
    const queue = new UploadQueue({ upload, maxBytes: () => 5 });
    queue.add([make("big.txt", 10), make("ok.txt", 3)]);
    expect(calls.map((c) => c.file.name)).toEqual(["ok.txt"]);
    const big = queue.getSnapshot()[0];
    expect(big?.state).toBe("failed");
    expect(big?.problem?.message).toContain("5 B");
  });

  it("dismisses finished uploads but never active ones", async () => {
    const { upload, calls } = manualUploader();
    const queue = new UploadQueue({ upload });
    const [a = "", b = ""] = queue.add([make("a.txt"), make("b.txt")]);
    calls[0]?.resolve({ created: true, file: fileItem("1", "a.txt") });
    await flush();
    queue.dismiss(b);
    expect(queue.getSnapshot()).toHaveLength(2);
    queue.dismiss(a);
    expect(queue.getSnapshot().map((i) => i.name)).toEqual(["b.txt"]);
    queue.clearFinished();
    expect(queue.getSnapshot()).toHaveLength(1);
  });
});
