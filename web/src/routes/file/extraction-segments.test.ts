import { describe, expect, it } from "vitest";
import { segmentByPages, splitAtPassage } from "./extraction-segments";

const page = (number: number, char_start: number, char_end: number) => ({
  number,
  char_start,
  char_end,
  source: "text" as const,
});

describe("segmentByPages", () => {
  it("records each segment's offset in the whole text", () => {
    const segs = segmentByPages("aaaabbbb", 10, [page(1, 10, 14), page(2, 14, 18)]);
    expect(segs.map((s) => [s.page, s.start, s.text])).toEqual([
      [1, 10, "aaaa"],
      [2, 14, "bbbb"],
    ]);
  });
});

describe("splitAtPassage", () => {
  it("cuts the passage out of a segment, counting code points", () => {
    expect(splitAtPassage({ text: "🦩 heron here", start: 100 }, { start: 102, end: 107 })).toEqual(
      [
        { text: "🦩 ", passage: false },
        { text: "heron", passage: true },
        { text: " here", passage: false },
      ],
    );
  });

  it("clamps passages that cross the segment and ignores ones outside it", () => {
    const seg = { text: "abcdef", start: 10 };
    expect(splitAtPassage(seg, { start: 5, end: 12 })).toEqual([
      { text: "ab", passage: true },
      { text: "cdef", passage: false },
    ]);
    expect(splitAtPassage(seg, { start: 20, end: 30 })).toEqual([
      { text: "abcdef", passage: false },
    ]);
    expect(splitAtPassage(seg, null)).toEqual([{ text: "abcdef", passage: false }]);
  });
});
