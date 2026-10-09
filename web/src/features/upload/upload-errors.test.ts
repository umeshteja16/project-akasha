import { describe, expect, it } from "vitest";
import { ApiError } from "@/api/client";
import { describeUploadError } from "./upload-errors";

describe("describeUploadError", () => {
  it.each([
    ["payload_too_large", 413, "Too large", false],
    ["quota_exceeded", 413, "Library is full", false],
    ["unsupported_media_type", 415, "Can't keep this type", false],
    ["unauthorized", 401, "Signed out", false],
    ["network", 0, "Connection lost", true],
    ["internal", 500, "Upload failed", true],
    ["rate_limited", 429, "Slow down", true],
  ])("%s → %s", (code, status, title, retryable) => {
    const problem = describeUploadError(new ApiError(status, code, "x"));
    expect(problem.title).toBe(title);
    expect(problem.retryable).toBe(retryable);
  });

  it("names the server limit when it is known", () => {
    const problem = describeUploadError(
      new ApiError(413, "payload_too_large", "x"),
      512 * 1024 * 1024,
    );
    expect(problem.message).toBe("This server accepts files up to 512 MB.");
  });

  it("sentence-cases validation messages from the server", () => {
    const problem = describeUploadError(new ApiError(400, "bad_request", "file name is empty"));
    expect(problem.message).toBe("File name is empty.");
  });

  it("handles errors that are not API errors", () => {
    expect(describeUploadError(new Error("boom")).title).toBe("Upload failed");
  });
});
