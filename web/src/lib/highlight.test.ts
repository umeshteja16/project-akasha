import { describe, expect, it } from "vitest";
import { codePointIndex, splitHighlights } from "./highlight";

describe("splitHighlights", () => {
  it("marks ASCII spans", () => {
    expect(splitHighlights("the heron flew", [{ start: 4, end: 9 }])).toEqual([
      { text: "the ", mark: false },
      { text: "heron", mark: true },
      { text: " flew", mark: false },
    ]);
  });

  it("counts offsets in code points, not UTF-16 units", () => {
    // "🦩" is one code point but two UTF-16 units.
    const text = "🦩 flamingo and 𝒽eron";
    expect(codePointIndex("a🦩b")).toEqual([0, 1, 3, 4]);
    const runs = splitHighlights(text, [
      { start: 2, end: 10 },
      { start: 15, end: 20 },
    ]);
    expect(runs.filter((r) => r.mark).map((r) => r.text)).toEqual(["flamingo", "𝒽eron"]);
    expect(runs.map((r) => r.text).join("")).toBe(text);
  });

  it("merges overlapping spans, clamps and drops empty ones", () => {
    const runs = splitHighlights("abcdef", [
      { start: 4, end: 99 },
      { start: 0, end: 2 },
      { start: 1, end: 3 },
      { start: 3, end: 3 },
    ]);
    expect(runs).toEqual([
      { text: "abc", mark: true },
      { text: "d", mark: false },
      { text: "ef", mark: true },
    ]);
  });

  it("keeps markup-like text as plain text", () => {
    const runs = splitHighlights("<img src=x onerror=alert(1)>", [{ start: 1, end: 4 }]);
    expect(runs.map((r) => r.text).join("")).toBe("<img src=x onerror=alert(1)>");
    expect(runs[1]).toEqual({ text: "img", mark: true });
  });

  it("handles no spans and empty text", () => {
    expect(splitHighlights("plain", [])).toEqual([{ text: "plain", mark: false }]);
    expect(splitHighlights("", [{ start: 0, end: 3 }])).toEqual([]);
  });
});
