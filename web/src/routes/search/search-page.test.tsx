import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App, createAppDeps } from "@/app";
import { apiError, fakeFetch, json, META, USER } from "@/test/fetch";

const BASE = "http://akasha.test";

function results(q: string, overrides: Record<string, unknown> = {}) {
  return {
    query: q,
    requested_mode: "hybrid",
    mode: "hybrid",
    degraded: false,
    reranked: true,
    warnings: [],
    has_more: false,
    suggestion: null,
    timings: {
      embed_ms: 1,
      keyword_ms: 2,
      semantic_ms: 3,
      fetch_ms: 1,
      rerank_ms: 4,
      total_ms: 11,
    },
    results: [
      {
        file: {
          id: "f0000000-0000-4000-8000-000000000001",
          name: "heron-notes.txt",
          mime_type: "text/plain",
          size_bytes: 10,
          status: "ready",
          tags: ["birds"],
          auto_tags: [],
          is_pinned: false,
          created_at: "2026-10-09T10:00:00Z",
          summary: "Notes from the pond.",
        },
        score: 0.03,
        match_count: 1,
        matches: [
          {
            chunk_id: 1,
            chunk_index: 0,
            char_start: 0,
            char_end: 60,
            page: null,
            scores: { fused: 0.03 },
            // "🦩" counts as one character on the server.
            snippet: { text: "🦩 The heron <b>returned</b>", highlights: [{ start: 6, end: 11 }] },
          },
        ],
      },
    ],
    ...overrides,
  };
}

function setup(path: string, search: (url: URL) => Response) {
  const queries: string[] = [];
  const { fetch } = fakeFetch({
    "GET /api/v1/meta": () => json(META),
    "GET /api/v1/me": () => json(USER),
    "GET /api/v1/tags": () => json({ items: [] }),
    "GET /api/v1/search": (req) => {
      const url = new URL(req.url);
      queries.push(url.search);
      return search(url);
    },
  });
  const history = createMemoryHistory({ initialEntries: [path] });
  const deps = createAppDeps({ fetch, baseUrl: BASE, history });
  render(<App deps={deps} />);
  return { deps, queries, history };
}

describe("search page", () => {
  it("renders results from the URL with safe highlights", async () => {
    const { queries } = setup("/search?q=heron&type=text", (url) =>
      json(results(url.searchParams.get("q") ?? "")),
    );
    const mark = await screen.findByText("heron", { selector: "mark" });
    expect(mark.parentElement?.textContent).toBe("🦩 The heron <b>returned</b>");
    expect(document.querySelector("b")).toBeNull();
    expect(queries[0]).toContain("q=heron");
    expect(queries[0]).toContain("type=text");
    expect(screen.getByRole("button", { name: "Text", pressed: true })).toBeInTheDocument();
    const link = screen.getByRole("link", { name: "heron-notes.txt" });
    expect(link.getAttribute("href")).toContain("at=0-60");
  });

  it("puts the typed query in the URL after a pause, searching once", async () => {
    const user = userEvent.setup();
    const { deps, queries } = setup("/search", (url) =>
      json(results(url.searchParams.get("q") ?? "")),
    );
    const box = await screen.findByRole("searchbox", { name: "Search your library" });
    await user.type(box, "heron pond");
    await waitFor(() => expect(deps.router.state.location.search).toEqual({ q: "heron pond" }));
    await screen.findByText("heron", { selector: "mark" });
    expect(queries).toHaveLength(1);

    // A filter change is a new history entry; back restores the previous state.
    await user.click(screen.getByRole("button", { name: "Pinned" }));
    await waitFor(() =>
      expect(deps.router.state.location.search).toEqual({ q: "heron pond", pinned: true }),
    );
    deps.router.history.back();
    await waitFor(() => expect(deps.router.state.location.search).toEqual({ q: "heron pond" }));
  });

  it("offers the spelling suggestion and warns when degraded", async () => {
    const user = userEvent.setup();
    const { deps } = setup("/search?q=hreon", (url) =>
      json(
        url.searchParams.get("q") === "hreon"
          ? results("hreon", { suggestion: "heron", degraded: true, mode: "keyword", results: [] })
          : results("heron"),
      ),
    );
    expect(await screen.findByText(/Showing keyword matches only/)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Nothing matched “hreon”" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "heron" }));
    await waitFor(() => expect(deps.router.state.location.search).toEqual({ q: "heron" }));
    expect(screen.getByRole("searchbox")).toHaveValue("heron");
  });

  it("explains a rate limit instead of failing", async () => {
    setup("/search?q=heron", () => apiError(429, "rate_limited", "too many searches, retry in 7s"));
    expect(await screen.findByText(/Results resume in about 7 s/)).toBeInTheDocument();
  });
});
