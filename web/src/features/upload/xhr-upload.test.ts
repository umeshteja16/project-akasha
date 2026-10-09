import { describe, expect, it } from "vitest";
import { isApiError } from "@/api/client";
import { UploadAborted, xhrUploader } from "./xhr-upload";

/** Just enough of XMLHttpRequest for the uploader. */
class FakeXhr {
  static last: FakeXhr | null = null;
  method = "";
  url = "";
  withCredentials = false;
  responseType = "";
  status = 0;
  responseText = "";
  headers: Record<string, string> = {};
  sent: FormData | null = null;
  upload: {
    onprogress: ((e: { loaded: number; total: number; lengthComputable: boolean }) => void) | null;
  } = {
    onprogress: null,
  };
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onabort: (() => void) | null = null;
  constructor() {
    FakeXhr.last = this;
  }
  open(method: string, url: string) {
    this.method = method;
    this.url = url;
  }
  setRequestHeader() {}
  getResponseHeader(name: string) {
    return this.headers[name] ?? null;
  }
  send(body: FormData) {
    this.sent = body;
  }
  abort() {
    this.onabort?.();
  }
  respond(status: number, body: unknown, headers: Record<string, string> = {}) {
    this.status = status;
    this.responseText = JSON.stringify(body);
    this.headers = headers;
    this.onload?.();
  }
}

const upload = xhrUploader("http://akasha.test", () => new FakeXhr() as unknown as XMLHttpRequest);
const file = new File(["hello"], "note.txt", { type: "text/plain" });
const xhr = () => {
  const last = FakeXhr.last;
  if (!last) throw new Error("no request");
  return last;
};

function start(signal = new AbortController().signal) {
  FakeXhr.last = null;
  const progress: number[] = [];
  const result = upload(file, { signal, onProgress: (loaded) => progress.push(loaded) });
  return { result, progress };
}

describe("xhrUploader", () => {
  it("posts the file as multipart with the cookie and reports progress", async () => {
    const { result, progress } = start();
    expect(xhr().method).toBe("POST");
    expect(xhr().url).toBe("http://akasha.test/api/v1/files");
    expect(xhr().withCredentials).toBe(true);
    const sent = xhr().sent?.get("file");
    expect(sent instanceof File && sent.name).toBe("note.txt");
    xhr().upload.onprogress?.({ loaded: 3, total: 5, lengthComputable: true });
    xhr().respond(201, { id: "f1", name: "note.txt" });
    await expect(result).resolves.toMatchObject({ created: true, file: { id: "f1" } });
    expect(progress).toEqual([3, 5]);
  });

  it("treats 200 as an existing file", async () => {
    const { result } = start();
    xhr().respond(200, { id: "f0", name: "older.txt" });
    await expect(result).resolves.toMatchObject({ created: false, file: { name: "older.txt" } });
  });

  it("turns error bodies into ApiErrors", async () => {
    const { result } = start();
    xhr().respond(415, { error: { code: "unsupported_media_type", message: "nope" } });
    const error = await result.catch((e: unknown) => e);
    expect(isApiError(error) && error.code).toBe("unsupported_media_type");
  });

  it("rejects with UploadAborted when cancelled", async () => {
    const controller = new AbortController();
    const { result } = start(controller.signal);
    controller.abort();
    await expect(result).rejects.toBeInstanceOf(UploadAborted);
  });

  it("reports network failures", async () => {
    const { result } = start();
    xhr().onerror?.();
    const error = await result.catch((e: unknown) => e);
    expect(isApiError(error) && error.code).toBe("network");
  });
});
