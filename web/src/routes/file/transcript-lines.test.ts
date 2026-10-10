import { describe, expect, it } from "vitest";
import { lineAt, linesInPassage, transcriptLines } from "./transcript-lines";

// "Héllo\n🎉 world!!\nbye": code points 0-4, 6-14, 16-18.
const segments = [
  { start_ms: 0, end_ms: 1500, char_start: 0, char_end: 5 },
  { start_ms: 1500, end_ms: 4000, char_start: 6, char_end: 15 },
  { start_ms: 9000, end_ms: 9500, char_start: 16, char_end: 19 },
];

describe("transcript lines", () => {
  it("cuts the text into timed lines across windows, by code point", () => {
    const lines = transcriptLines(
      [
        { offset: 0, text: "Héllo\n🎉 world" },
        { offset: 13, text: "!!\nbye" },
      ],
      segments,
    );
    expect(lines.map((l) => l.text)).toEqual(["Héllo", "🎉 world!!", "bye"]);
    expect(lines.map((l) => l.startMs)).toEqual([0, 1500, 9000]);
  });

  it("leaves out lines that are not loaded yet", () => {
    expect(transcriptLines([{ offset: 0, text: "Héllo\n🎉 wor" }], segments)).toHaveLength(1);
  });

  it("finds the line at a time and the lines of a passage", () => {
    const lines = transcriptLines([{ offset: 0, text: "Héllo\n🎉 world!!\nbye" }], segments);
    expect(lineAt(lines, 0)).toBe(0);
    expect(lineAt(lines, 3000)).toBe(1);
    expect(lineAt(lines, 8000)).toBe(1);
    expect(lineAt(lines, 99_000)).toBe(2);
    expect(lineAt([], 5)).toBe(-1);
    expect([...linesInPassage(lines, { start: 7, end: 12 })]).toEqual([1]);
    expect(linesInPassage(lines, null).size).toBe(0);
  });
});
