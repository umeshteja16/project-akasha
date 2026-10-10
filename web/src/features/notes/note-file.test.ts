import { describe, expect, it } from "vitest";
import { noteFile } from "./note-file";

const read = (file: File) =>
  new Promise<string>((resolve) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.readAsText(file);
  });

const NOW = new Date("2026-10-10T09:05:30Z");

describe("noteFile", () => {
  it("names the file after the first line and the time", async () => {
    const file = noteFile("# Ideas: for / later\nmore", NOW);
    expect(file.name).toBe("Ideas for  later 2026-10-10 09-05.md");
    expect(file.type).toBe("text/markdown");
    expect(await read(file)).toBe("# Ideas: for / later\nmore\n");
  });

  it("falls back to Note when the first line has no usable characters", () => {
    expect(noteFile("???", NOW).name).toBe("Note 2026-10-10 09-05.md");
  });
});
