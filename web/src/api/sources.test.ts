import { describe, expect, it } from "vitest";
import { parseGlobs, type Source, sourceState } from "./sources";

const base: Source = {
  id: "00000000-0000-0000-0000-000000000001",
  kind: "folder",
  name: "Vault",
  path: "/data/notes/Vault",
  include_globs: [],
  exclude_globs: [],
  on_delete: "delete",
  import_tags: true,
  enabled: true,
  status: "ok",
  last_error: null,
  last_scan_at: null,
  last_scan: { files: 0, imported: 0, updated: 0, removed: 0, skipped: 0 },
  file_count: 0,
  skipped_count: 0,
  created_at: "2026-10-10T00:00:00Z",
};

describe("sources", () => {
  it("reads one glob per line", () => {
    expect(parseGlobs(" **/*.md \n\nArchive/**\n")).toEqual(["**/*.md", "Archive/**"]);
  });

  it("summarises the sync state", () => {
    expect(sourceState(base)).toBe("ok");
    expect(sourceState({ ...base, status: "pending" })).toBe("syncing");
    expect(sourceState({ ...base, status: "scanning" })).toBe("syncing");
    expect(sourceState({ ...base, status: "error" })).toBe("error");
    expect(sourceState({ ...base, status: "scanning", enabled: false })).toBe("paused");
  });
});
