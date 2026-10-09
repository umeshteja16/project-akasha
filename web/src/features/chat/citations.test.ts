import { describe, expect, it } from "vitest";
import { splitCitations, stripCitations } from "./citations";

const known = (n: number) => n >= 1 && n <= 3;

describe("splitCitations", () => {
  it("turns known [n] markers into citations", () => {
    expect(splitCitations("Herons nest early [1]. They fish at dawn [2].", known)).toEqual([
      "Herons nest early ",
      { n: 1 },
      ". They fish at dawn ",
      { n: 2 },
      ".",
    ]);
  });

  it("handles lists and adjacent markers", () => {
    expect(splitCitations("Both [1, 3] and [2][3]", known)).toEqual([
      "Both ",
      { n: 1 },
      { n: 3 },
      " and ",
      { n: 2 },
      { n: 3 },
    ]);
  });

  it("leaves unknown numbers and other brackets as text", () => {
    expect(splitCitations("See [7], [a] and [1, 9] or arr[0]", known)).toEqual([
      "See [7], [a] and [1, 9] or arr[0]",
    ]);
  });

  it("never interprets markup", () => {
    const pieces = splitCitations("<b>bold</b> [1]", known);
    expect(pieces[0]).toBe("<b>bold</b> ");
  });
});

describe("stripCitations", () => {
  it("removes markers for plain copying", () => {
    expect(stripCitations("Herons nest early [1]. Fish [2, 3].")).toBe("Herons nest early. Fish.");
  });
});
