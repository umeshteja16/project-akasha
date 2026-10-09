import { describe, expect, it } from "vitest";
import { parsePassage, passageSearch, validateFileSearch } from "./passage";

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
});
