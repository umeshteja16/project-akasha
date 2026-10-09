// The upload queue: a small external store (for `useSyncExternalStore`) that
// runs a few uploads at a time, tracks per-file progress and lets each one be
// cancelled, retried or dismissed. Framework-free so it is easy to test.

import type { FileItem } from "@/api/files";
import { describeUploadError, tooLargeLocally, type UploadProblem } from "./upload-errors";
import { UploadAborted, type Uploader, type UploadResult } from "./xhr-upload";

export type UploadState =
  | "queued"
  | "uploading"
  /** Stored as a new file. */
  | "done"
  /** The same bytes were already in the library. */
  | "duplicate"
  | "failed"
  | "cancelled";

export interface UploadItem {
  id: string;
  name: string;
  size: number;
  loaded: number;
  state: UploadState;
  /** The stored (or already existing) file, once the server answered. */
  file?: FileItem;
  problem?: UploadProblem;
}

export interface UploadQueueOptions {
  upload: Uploader;
  /** Uploads running at once. */
  concurrency?: number;
  /** Largest accepted file, checked before sending (`null`: unknown). */
  maxBytes?: () => number | null;
  /** Called once per finished upload (new or duplicate). */
  onUploaded?: (result: UploadResult) => void;
}

const ACTIVE: ReadonlySet<UploadState> = new Set(["queued", "uploading"]);

export function isActive(item: UploadItem): boolean {
  return ACTIVE.has(item.state);
}

/** Overall progress of the active and finished uploads, 0..1. */
export function overallProgress(items: readonly UploadItem[]): number {
  const counted = items.filter((i) => i.state !== "cancelled" && i.state !== "failed");
  const total = counted.reduce((sum, i) => sum + i.size, 0);
  if (total === 0) return counted.length > 0 && counted.every((i) => !isActive(i)) ? 1 : 0;
  return counted.reduce((sum, i) => sum + Math.min(i.loaded, i.size), 0) / total;
}

let counter = 0;

export class UploadQueue {
  private items: readonly UploadItem[] = [];
  private readonly files = new Map<string, File>();
  private readonly controllers = new Map<string, AbortController>();
  private readonly listeners = new Set<() => void>();
  private readonly options: Required<Omit<UploadQueueOptions, "onUploaded">> &
    Pick<UploadQueueOptions, "onUploaded">;

  constructor(options: UploadQueueOptions) {
    this.options = { concurrency: 3, maxBytes: () => null, ...options };
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): readonly UploadItem[] => this.items;

  /** Queue files; returns their upload ids. */
  add(files: Iterable<File>): string[] {
    const ids: string[] = [];
    const added: UploadItem[] = [];
    const max = this.options.maxBytes();
    for (const file of files) {
      counter += 1;
      const id = `upload-${counter}`;
      ids.push(id);
      this.files.set(id, file);
      const item: UploadItem = { id, name: file.name, size: file.size, loaded: 0, state: "queued" };
      if (max !== null && file.size > max) {
        item.state = "failed";
        item.problem = describeUploadError(tooLargeLocally(), max);
      }
      added.push(item);
    }
    if (added.length === 0) return ids;
    this.items = [...this.items, ...added];
    this.emit();
    this.pump();
    return ids;
  }

  cancel(id: string): void {
    const item = this.find(id);
    if (!item || !isActive(item)) return;
    this.controllers.get(id)?.abort();
    this.patch(id, { state: "cancelled" });
    this.pump();
  }

  cancelAll(): void {
    for (const item of this.items) if (isActive(item)) this.cancel(item.id);
  }

  /** Try a failed or cancelled upload again. */
  retry(id: string): void {
    const item = this.find(id);
    if (!item || (item.state !== "failed" && item.state !== "cancelled")) return;
    const max = this.options.maxBytes();
    if (max !== null && item.size > max) return;
    this.patch(id, { state: "queued", loaded: 0, problem: undefined });
    this.pump();
  }

  /** Remove a finished upload from the list. */
  dismiss(id: string): void {
    const item = this.find(id);
    if (!item || isActive(item)) return;
    this.items = this.items.filter((i) => i.id !== id);
    this.files.delete(id);
    this.emit();
  }

  /** Remove every finished upload. */
  clearFinished(): void {
    const keep = this.items.filter(isActive);
    for (const item of this.items) if (!isActive(item)) this.files.delete(item.id);
    this.items = keep;
    this.emit();
  }

  private find(id: string): UploadItem | undefined {
    return this.items.find((i) => i.id === id);
  }

  private patch(id: string, changes: Partial<UploadItem>): void {
    this.items = this.items.map((i) => (i.id === id ? { ...i, ...changes } : i));
    this.emit();
  }

  private emit(): void {
    for (const listener of this.listeners) listener();
  }

  private pump(): void {
    let running = this.items.filter((i) => i.state === "uploading").length;
    for (const item of this.items) {
      if (running >= this.options.concurrency) break;
      if (item.state !== "queued") continue;
      running += 1;
      void this.run(item.id);
    }
  }

  private async run(id: string): Promise<void> {
    const file = this.files.get(id);
    if (!file) return;
    const controller = new AbortController();
    this.controllers.set(id, controller);
    this.patch(id, { state: "uploading", loaded: 0 });
    try {
      const result = await this.options.upload(file, {
        signal: controller.signal,
        onProgress: (loaded) => {
          if (this.find(id)?.state === "uploading") this.patch(id, { loaded });
        },
      });
      if (this.find(id)?.state !== "uploading") return;
      this.patch(id, {
        state: result.created ? "done" : "duplicate",
        loaded: file.size,
        file: result.file,
      });
      this.options.onUploaded?.(result);
    } catch (error) {
      if (error instanceof UploadAborted || this.find(id)?.state !== "uploading") return;
      this.patch(id, {
        state: "failed",
        problem: describeUploadError(error, this.options.maxBytes()),
      });
    } finally {
      this.controllers.delete(id);
      this.pump();
    }
  }
}
