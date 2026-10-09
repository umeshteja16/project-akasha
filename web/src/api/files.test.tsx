import { QueryClient, QueryClientProvider, useInfiniteQuery } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";
import { fakeFetch, json } from "@/test/fetch";
import { createApi } from "./client";
import { awaitingEnrichment, type FileItem, filesQuery, pollWhileProcessing } from "./files";

function fileItem(status: FileItem["status"], overrides: Partial<FileItem> = {}): FileItem {
  return {
    id: "f1",
    name: "note.txt",
    mime_type: "text/plain",
    size_bytes: 5,
    status,
    content_hash: "abc",
    is_pinned: false,
    tags: [],
    auto_tags: [],
    created_at: "2026-10-09T10:00:00Z",
    updated_at: "2026-10-09T10:00:00Z",
    ...overrides,
  };
}

describe("pollWhileProcessing", () => {
  it("polls only while a visible file is not terminal", () => {
    expect(pollWhileProcessing(undefined)).toBe(false);
    expect(pollWhileProcessing([])).toBe(false);
    expect(pollWhileProcessing([{ status: "ready" }, { status: "failed" }])).toBe(false);
    expect(pollWhileProcessing([{ status: "ready" }, { status: "pending" }], 50)).toBe(50);
    expect(pollWhileProcessing([{ status: "processing" }], 50)).toBe(50);
  });
});

describe("awaitingEnrichment", () => {
  const now = Date.parse("2026-10-09T10:00:30Z");
  it("waits briefly for a summary after a file becomes ready", () => {
    expect(awaitingEnrichment(fileItem("ready"), now)).toBe(true);
    expect(awaitingEnrichment(fileItem("ready"), now + 120_000)).toBe(false);
    expect(awaitingEnrichment(fileItem("processing"), now)).toBe(false);
    const enriched = fileItem("ready", {
      enrichment: { status: "done", model: "fake", updated_at: "2026-10-09T10:00:20Z" },
    });
    expect(awaitingEnrichment(enriched, now)).toBe(false);
  });
});

describe("filesQuery", () => {
  it("refetches while files process and stops once they are ready", async () => {
    const states: FileItem["status"][] = ["pending", "processing", "ready"];
    let calls = 0;
    const { fetch } = fakeFetch({
      "GET /api/v1/files": () => {
        const status = states[Math.min(calls, states.length - 1)] ?? "ready";
        calls += 1;
        return json({ items: [fileItem(status)], next_cursor: null });
      },
    });
    const api = createApi({ baseUrl: "http://akasha.test", fetch });
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(() => useInfiniteQuery(filesQuery(api, {}, "newest", 10)), {
      wrapper,
    });
    await waitFor(() => expect(result.current.data?.pages[0]?.items[0]?.status).toBe("ready"));
    const settled = calls;
    expect(settled).toBe(3);
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(calls).toBe(settled);
    client.clear();
  });
});
