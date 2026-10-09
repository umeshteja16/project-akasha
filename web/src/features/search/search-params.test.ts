import { describe, expect, it } from "vitest";
import {
  activeFilters,
  clearFilters,
  daysAgo,
  toApiQuery,
  updateParams,
  validateSearchParams,
} from "./search-params";

describe("validateSearchParams", () => {
  it("keeps valid values and drops defaults and junk", () => {
    expect(
      validateSearchParams({
        q: "  heron notes ",
        mode: "hybrid",
        type: "pdf",
        from: "2026-01-31",
        to: "not a date",
        tags: " Work,q3,,work ",
        pinned: "true",
        page: "3",
        extra: "x",
      }),
    ).toEqual({
      q: "heron notes",
      type: "pdf",
      from: "2026-01-31",
      tags: "work,q3",
      pinned: true,
      page: 3,
    });
  });

  it("rejects unknown modes and types, bad pages, blank queries", () => {
    expect(
      validateSearchParams({ q: "   ", mode: "magic", type: "doc", page: 1, pinned: "no" }),
    ).toEqual({});
    expect(validateSearchParams({ page: 2.5 })).toEqual({});
    expect(validateSearchParams({ page: 99 })).toEqual({});
    expect(validateSearchParams({ mode: "semantic" })).toEqual({ mode: "semantic" });
  });

  it("swaps a reversed date range and accepts tag arrays", () => {
    expect(
      validateSearchParams({ from: "2026-03-01", to: "2026-01-01", tags: ["A", "b"] }),
    ).toEqual({ from: "2026-01-01", to: "2026-03-01", tags: "a,b" });
  });

  it("caps very long queries", () => {
    expect(validateSearchParams({ q: "x".repeat(900) }).q).toHaveLength(500);
  });
});

describe("updateParams", () => {
  it("resets the page unless it is part of the change, and drops unset keys", () => {
    const current = { q: "heron", page: 3, type: "pdf" as const };
    expect(updateParams(current, { type: undefined })).toEqual({ q: "heron" });
    expect(updateParams(current, { page: 4 })).toEqual({ q: "heron", page: 4, type: "pdf" });
    expect(updateParams(current, { page: 1 })).toEqual({ q: "heron", type: "pdf" });
    expect(updateParams(current, { q: "" })).toEqual({ type: "pdf" });
  });

  it("counts and clears filters but keeps the query and mode", () => {
    const p = { q: "x", mode: "keyword" as const, type: "pdf" as const, from: "2026-01-01" };
    expect(activeFilters(p)).toBe(2);
    expect(clearFilters(p)).toEqual({ q: "x", mode: "keyword" });
  });
});

describe("toApiQuery", () => {
  it("maps URL state onto the API's query parameters", () => {
    expect(toApiQuery({ q: "heron", pinned: true, tags: "a,b", page: 2 }, 10)).toEqual({
      q: "heron",
      limit: 10,
      pinned: true,
      tags: "a,b",
      page: 2,
    });
  });
});

describe("daysAgo", () => {
  it("formats local days", () => {
    expect(daysAgo(7, new Date(2026, 9, 9, 15))).toBe("2026-10-02");
    expect(daysAgo(0, new Date(2026, 0, 5))).toBe("2026-01-05");
  });
});
