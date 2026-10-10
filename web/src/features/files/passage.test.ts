import { describe, expect, it } from "vitest";
import { formatTimestamp, parsePassage, passageSearch, validateFileSearch } from "./passage";

describe("passage links", () => {
  it("round-trips a passage and page", () => {
    const search = passageSearch(120, 480, 3);
    expect(search).toEqual({ at: "120-480", page: 3 });
    expect(validateFileSearch({ ...search })).toEqual(search);
    expect(parsePassage(search.at)).toEqual({ start: 120, end: 480 });
    expect(passageSearch(0, 10, null)).toEqual({ at: "0-10" });
  });

  it("drops malformed values", () => {
    expect(validateFileSearch({ at: "10-5", page: "0" })).toEqual({});
    expect(validateFileSearch({ at: "<script>", page: "x" })).toEqual({});
    expect(validateFileSearch({ at: "5-10", page: "2" })).toEqual({ at: "5-10", page: 2 });
  });

  it("links into recordings at a second", () => {
    const search = passageSearch(0, 40, null, 65_400);
    expect(search).toEqual({ at: "0-40", t: 65 });
    expect(validateFileSearch({ ...search, t: "65" })).toEqual(search);
    expect(validateFileSearch({ t: "-1" })).toEqual({});
    expect(validateFileSearch({ t: "1.5" })).toEqual({});
  });

  it("formats times like clocks", () => {
    expect(formatTimestamp(0)).toBe("0:00");
    expect(formatTimestamp(65_400)).toBe("1:05");
    expect(formatTimestamp(3_723_000)).toBe("1:02:03");
  });
});
