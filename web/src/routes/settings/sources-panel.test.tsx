import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Source } from "@/api/sources";
import { App, createAppDeps } from "@/app";
import { fakeFetch, json, META, USER } from "@/test/fetch";

const VAULT: Source = {
  id: "11111111-1111-4111-8111-111111111111",
  kind: "folder",
  name: "Vault",
  path: "/data/notes/Vault",
  include_globs: [],
  exclude_globs: [],
  on_delete: "delete",
  import_tags: true,
  enabled: true,
  status: "error",
  last_error: "the folder is empty, so its 3 imported files were kept; is the volume mounted?",
  last_scan_at: "2026-10-10T08:00:00Z",
  last_scan: { files: 3, imported: 3, updated: 0, removed: 0, skipped: 0 },
  file_count: 3,
  skipped_count: 1,
  created_at: "2026-10-01T10:00:00Z",
};

function app(handlers: Parameters<typeof fakeFetch>[0]) {
  const { fetch, calls } = fakeFetch({
    "GET /api/v1/meta": () => json(META),
    "GET /api/v1/me": () => json(USER),
    ...handlers,
  });
  const history = createMemoryHistory({ initialEntries: ["/settings?tab=sources"] });
  render(<App deps={createAppDeps({ fetch, baseUrl: "http://akasha.test", history })} />);
  return calls;
}

describe("settings → sources", () => {
  it("adds a folder under the allowed root and lists folders with their state", async () => {
    let added: unknown = null;
    const calls = app({
      "GET /api/v1/sources": () => json({ items: [VAULT], roots: ["/data/notes"] }),
      "POST /api/v1/sources": async (req) => {
        added = await req.json();
        return json({ ...VAULT, id: "2", name: "Docs", status: "pending" }, 201);
      },
    });
    const list = await screen.findByRole("list", { name: "Watched folders" });
    expect(within(list).getByText("Vault")).toBeInTheDocument();
    expect(within(list).getByText("Problem")).toBeInTheDocument();
    expect(within(list).getByText(/is the volume mounted/)).toBeInTheDocument();
    expect(within(list).getByText(/3 files · 1 skipped/)).toBeInTheDocument();

    const user = userEvent.setup();
    const path = screen.getByLabelText("Folder on the server");
    expect(path).toHaveValue("/data/notes/");
    await user.type(path, "Docs/");
    await user.click(screen.getByRole("radio", { name: "Keep it" }));
    await user.click(screen.getByRole("button", { name: "Watch folder" }));
    await screen.findByText(/Watching “Docs”/);
    expect(added).toEqual({
      path: "/data/notes/Docs",
      name: null,
      on_delete: "keep",
      include_globs: [],
      exclude_globs: [],
    });
    expect(calls).toContain("POST /api/v1/sources");
  });

  it("explains how to turn the feature on when the server has no roots", async () => {
    app({ "GET /api/v1/sources": () => json({ items: [], roots: [] }) });
    expect(await screen.findByText(/Watched folders are off on this server/)).toBeInTheDocument();
    expect(screen.getByText("No watched folders yet.")).toBeInTheDocument();
  });
});
