// One file upload over XMLHttpRequest: fetch cannot report upload progress in
// every browser, XHR can (`xhr.upload.onprogress`) and can be aborted.

import { ApiError, errorFromBody, networkError } from "@/api/client";
import type { FileItem } from "@/api/files";

export interface UploadResult {
  /** `false`: the same bytes were already in the library (HTTP 200). */
  created: boolean;
  file: FileItem;
}

export interface UploadOptions {
  onProgress: (loaded: number, total: number) => void;
  signal: AbortSignal;
}

export type Uploader = (file: File, options: UploadOptions) => Promise<UploadResult>;

export class UploadAborted extends Error {
  constructor() {
    super("upload cancelled");
    this.name = "UploadAborted";
  }
}

function parse(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

/** An uploader posting to `${baseUrl}/api/v1/files` with the session cookie. */
export function xhrUploader(
  baseUrl = "",
  createXhr: () => XMLHttpRequest = () => new XMLHttpRequest(),
): Uploader {
  return (file, { onProgress, signal }) =>
    new Promise<UploadResult>((resolve, reject) => {
      if (signal.aborted) {
        reject(new UploadAborted());
        return;
      }
      const xhr = createXhr();
      xhr.open("POST", `${baseUrl}/api/v1/files`);
      xhr.withCredentials = true;
      xhr.responseType = "text";
      xhr.setRequestHeader("accept", "application/json");
      const abort = () => xhr.abort();
      signal.addEventListener("abort", abort, { once: true });
      const done = () => signal.removeEventListener("abort", abort);

      xhr.upload.onprogress = (event) => {
        onProgress(event.loaded, event.lengthComputable ? event.total : file.size);
      };
      xhr.onload = () => {
        done();
        const body = parse(xhr.responseText);
        if (xhr.status === 200 || xhr.status === 201) {
          if (body && typeof body === "object" && "id" in body) {
            onProgress(file.size, file.size);
            resolve({ created: xhr.status === 201, file: body as FileItem });
          } else {
            reject(
              new ApiError(xhr.status, "bad_response", "The server sent an unexpected reply."),
            );
          }
          return;
        }
        reject(errorFromBody(xhr.status, body, xhr.getResponseHeader("retry-after")));
      };
      xhr.onerror = () => {
        done();
        reject(networkError(new Error("upload failed")));
      };
      xhr.onabort = () => {
        done();
        reject(new UploadAborted());
      };

      const form = new FormData();
      form.append("file", file, file.name);
      xhr.send(form);
    });
}
