import { describe, expect, it } from "vitest";
import { describeAgent } from "./user-agent";

describe("describeAgent", () => {
  it("names common browsers and systems", () => {
    expect(
      describeAgent(
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 14.6; rv:140.0) Gecko/20100101 Firefox/140.0",
      ),
    ).toBe("Firefox on macOS");
    expect(
      describeAgent(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0 Safari/537.36 Edg/141.0",
      ),
    ).toBe("Edge on Windows");
    expect(
      describeAgent(
        "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
      ),
    ).toBe("Safari on iOS");
    expect(describeAgent("curl/8.5.0")).toBe("curl");
    expect(describeAgent(null)).toBe("Unknown browser");
    expect(describeAgent("x".repeat(60))).toHaveLength(41);
  });
});
