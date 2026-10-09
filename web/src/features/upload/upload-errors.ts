// Upload failures in plain words, keyed by the API's error codes.

import { ApiError, isApiError } from "@/api/client";
import { formatBytes } from "@/lib/format";

export interface UploadProblem {
  /** Short heading: "Too large". */
  title: string;
  /** One sentence on what happened and what to do. */
  message: string;
  /** Trying the same file again could work (network trouble, server hiccup). */
  retryable: boolean;
}

function sentence(text: string): string {
  const trimmed = text.trim();
  if (!trimmed) return trimmed;
  const capital = trimmed.charAt(0).toUpperCase() + trimmed.slice(1);
  return /[.!?]$/.test(capital) ? capital : `${capital}.`;
}

/** Map any upload error to what the person should read. */
export function describeUploadError(error: unknown, maxBytes: number | null = null): UploadProblem {
  if (!isApiError(error)) {
    return {
      title: "Upload failed",
      message: "Something unexpected went wrong. Try again.",
      retryable: true,
    };
  }
  switch (error.code) {
    case "payload_too_large":
      return {
        title: "Too large",
        message: maxBytes
          ? `This server accepts files up to ${formatBytes(maxBytes)}.`
          : "This file is larger than this server accepts.",
        retryable: false,
      };
    case "quota_exceeded":
      return {
        title: "Library is full",
        message: "There's no room left in your storage quota. Delete files you no longer need.",
        retryable: false,
      };
    case "unsupported_media_type":
      return {
        title: "Can't keep this type",
        message: "Akasha keeps PDFs, images, audio, video and text files (.txt, .md, .csv, .json).",
        retryable: false,
      };
    case "unauthorized":
      return {
        title: "Signed out",
        message: "Your session ended. Sign in again, then retry.",
        retryable: false,
      };
    case "network":
      return {
        title: "Connection lost",
        message: "The upload was interrupted. Check your connection and retry.",
        retryable: true,
      };
    case "bad_request":
      return { title: "Not accepted", message: sentence(error.message), retryable: false };
    default:
      if (error.status === 429) {
        return {
          title: "Slow down",
          message: "Too many uploads at once. Wait a moment and retry.",
          retryable: true,
        };
      }
      return {
        title: "Upload failed",
        message:
          error.status >= 500
            ? "The server couldn't store this file. Try again in a moment."
            : sentence(error.message),
        retryable: error.status >= 500 || error.status === 0,
      };
  }
}

/** The error the client raises before sending a file the server would refuse. */
export function tooLargeLocally(): ApiError {
  return new ApiError(413, "payload_too_large", "file too large");
}
