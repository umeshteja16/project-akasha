import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App, createAppDeps } from "@/app";
import { fakeFetch, json, META, USER } from "@/test/fetch";

const CONV = "c0000000-0000-4000-8000-000000000001";
const FILE = "f0000000-0000-4000-8000-000000000001";
const AT = Date.parse("2026-10-09T10:00:00Z");

function conversation(fileIds: string[]) {
  return {
    id: CONV,
    title: "Lease questions",
    file_ids: fileIds,
    created_at: "2026-10-09T10:00:00Z",
    updated_at: "2026-10-09T10:00:00Z",
  };
}

/** Message `i` (0 = oldest): questions at even, answers at odd positions. */
function message(i: number) {
  const user = i % 2 === 0;
  return {
    id: `m${String(i).padStart(4, "0")}`,
    role: user ? "user" : "assistant",
    content: user ? `Question ${i}` : `Answer ${i}`,
    status: "answered",
    citations: [],
    model: user ? null : "fake/fake-echo",
    usage: {},
    latency_ms: null,
    created_at: new Date(AT + i * 1000).toISOString(),
  };
}

function setup(fileIds: string[] = []) {
  const total = 120;
  const patches: unknown[] = [];
  let current = conversation(fileIds);
  const { fetch } = fakeFetch({
    "GET /api/v1/meta": () => json(META),
    "GET /api/v1/me": () => json(USER),
    "GET /api/v1/conversations": () => json({ items: [current], next_cursor: null }),
    [`GET /api/v1/conversations/${CONV}`]: () => json(current),
    [`PATCH /api/v1/conversations/${CONV}`]: async (req) => {
      const body = (await req.json()) as { file_ids?: string[] };
      patches.push(body);
      current = { ...current, file_ids: body.file_ids ?? current.file_ids };
      return json(current);
    },
    // Keyset pages of 50, newest page first; the cursor is the index to stop before.
    [`GET /api/v1/conversations/${CONV}/messages`]: (req) => {
      const cursor = new URL(req.url).searchParams.get("cursor");
      const end = cursor ? Number(cursor) : total;
      const start = Math.max(0, end - 50);
      const items = Array.from({ length: end - start }, (_, k) => message(start + k));
      return json({ items, next_cursor: start > 0 ? String(start) : null });
    },
    [`GET /api/v1/files/${FILE}`]: () =>
      json({
        id: FILE,
        name: "lease.md",
        mime_type: "text/markdown",
        size_bytes: 10,
        status: "ready",
        tags: [],
        auto_tags: [],
        is_pinned: false,
        created_at: "2026-10-09T10:00:00Z",
        updated_at: "2026-10-09T10:00:00Z",
      }),
  });
  const history = createMemoryHistory({ initialEntries: [`/chat/${CONV}`] });
  render(<App deps={createAppDeps({ fetch, baseUrl: "http://akasha.test", history })} />);
  return { patches };
}

describe("conversation page", () => {
  it("pages back through more than 50 messages", async () => {
    const user = userEvent.setup();
    setup();
    expect(await screen.findByText("Question 118")).toBeInTheDocument();
    expect(screen.queryByText("Question 68")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Earlier messages" }));
    expect(await screen.findByText("Question 68")).toBeInTheDocument();
    expect(screen.queryByText("Question 0")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Earlier messages" }));
    expect(await screen.findByText("Question 0")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Earlier messages" })).toBeNull();
    // Oldest first on screen.
    const texts = screen.getAllByText(/^Question \d+$/).map((el) => el.textContent);
    expect(texts[0]).toBe("You asked: Question 0");
    expect(texts.at(-1)).toBe("You asked: Question 118");
  });

  it("edits an earlier question into the composer", async () => {
    const user = userEvent.setup();
    setup();
    const asked = await screen.findByText("Question 118");
    const row = asked.closest("div");
    if (!row) throw new Error("no question row");
    await user.click(within(row).getByRole("button", { name: "Edit and ask again" }));
    const box = screen.getByRole("textbox", { name: "Your question" });
    expect(box).toHaveValue("Question 118");
    expect(box).toHaveFocus();
  });

  it("shows the stored file scope and clears it on the server", async () => {
    const user = userEvent.setup();
    const { patches } = setup([FILE]);
    expect(await screen.findByText("lease.md")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Ask the whole library instead" }));
    await waitFor(() => expect(patches).toEqual([{ file_ids: [] }]));
    await waitFor(() => expect(screen.queryByText(/Answering only from/)).toBeNull());
  });
});
