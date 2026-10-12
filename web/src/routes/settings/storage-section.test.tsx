import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App, createAppDeps } from "@/app";
import { fakeFetch, json, META, USER } from "@/test/fetch";
import { parseLimitGb, toLimitGb } from "./storage-section";

const GB = 1024 ** 3;

function withStorage(storage: {
  used_bytes: number;
  quota_bytes: number | null;
  remaining_bytes: number | null;
}) {
  return { ...USER, storage };
}

function renderSettings(me: unknown, patch?: (body: unknown) => unknown) {
  const bodies: unknown[] = [];
  const { fetch } = fakeFetch({
    "GET /api/v1/meta": () => json(META),
    "GET /api/v1/me": () => json(me),
    "PATCH /api/v1/me": async (req: Request) => {
      const body = await req.json();
      bodies.push(body);
      return json(patch ? patch(body) : me);
    },
  });
  const history = createMemoryHistory({ initialEntries: ["/settings"] });
  render(<App deps={createAppDeps({ fetch, baseUrl: "http://akasha.test", history })} />);
  return bodies;
}

describe("settings → storage", () => {
  it("shows what is stored when no limit is set", async () => {
    renderSettings(withStorage({ used_bytes: 5 * GB, quota_bytes: null, remaining_bytes: null }));
    expect(await screen.findByText("5 GB stored, with no limit set.")).toBeInTheDocument();
    expect(screen.queryByRole("progressbar", { name: "Storage used" })).not.toBeInTheDocument();
  });

  it("shows a bar and what is left against a limit", async () => {
    renderSettings(
      withStorage({ used_bytes: 2 * GB, quota_bytes: 8 * GB, remaining_bytes: 6 * GB }),
    );
    expect(await screen.findByText(/2 GB of 8 GB used/)).toBeInTheDocument();
    expect(screen.getByText(/6 GB left/)).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "Storage used" })).toHaveAttribute(
      "aria-valuenow",
      "25",
    );
  });

  it("warns when usage is over the limit, without claiming anything was deleted", async () => {
    renderSettings(withStorage({ used_bytes: 9 * GB, quota_bytes: 4 * GB, remaining_bytes: 0 }));
    expect(await screen.findByText(/You are over your limit/)).toBeInTheDocument();
    expect(screen.getByText(/Nothing was deleted/)).toBeInTheDocument();
    expect(screen.queryByText(/left/)).not.toBeInTheDocument();
  });

  it("saves a limit in GB as bytes", async () => {
    const me = withStorage({ used_bytes: 0, quota_bytes: null, remaining_bytes: null });
    const bodies = renderSettings(me, () => ({
      ...me,
      storage: { used_bytes: 0, quota_bytes: 2.5 * GB, remaining_bytes: 2.5 * GB },
    }));
    const box = await screen.findByLabelText("Limit (GB)");
    await userEvent.type(box, "2.5");
    await userEvent.click(screen.getByRole("button", { name: "Save limit" }));
    await waitFor(() => expect(bodies).toEqual([{ storage_quota_bytes: 2.5 * GB }]));
    expect(await screen.findByText(/0 B of 2.5 GB used/)).toBeInTheDocument();
  });

  it("clears the limit when the box is emptied", async () => {
    const me = withStorage({ used_bytes: GB, quota_bytes: 4 * GB, remaining_bytes: 3 * GB });
    const bodies = renderSettings(me, () => ({
      ...me,
      storage: { used_bytes: GB, quota_bytes: null, remaining_bytes: null },
    }));
    const box = await screen.findByLabelText("Limit (GB)");
    expect(box).toHaveValue(4);
    await userEvent.clear(box);
    await userEvent.click(screen.getByRole("button", { name: "Save limit" }));
    await waitFor(() => expect(bodies).toEqual([{ storage_quota_bytes: null }]));
  });

  it("refuses a negative limit without sending anything", async () => {
    const bodies = renderSettings(
      withStorage({ used_bytes: 0, quota_bytes: null, remaining_bytes: null }),
    );
    const box = await screen.findByLabelText("Limit (GB)");
    await userEvent.type(box, "-5");
    expect(screen.getByText("Enter a size in GB, or leave it empty.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save limit" })).toBeDisabled();
    expect(bodies).toEqual([]);
  });
});

describe("storage limit parsing", () => {
  it("round-trips bytes through the GB box", () => {
    expect(toLimitGb(null)).toBe("");
    expect(toLimitGb(undefined)).toBe("");
    expect(toLimitGb(2 * GB)).toBe("2");
    expect(toLimitGb(GB / 2)).toBe("0.5");
    // Big limits lose the decimals rather than show 1023.99.
    expect(toLimitGb(1024 * GB - 1)).toBe("1024");
    expect(parseLimitGb("")).toBeNull();
    expect(parseLimitGb("  ")).toBeNull();
    expect(parseLimitGb("2")).toBe(2 * GB);
    expect(parseLimitGb("0.5")).toBe(GB / 2);
    expect(parseLimitGb("0")).toBe(0);
    expect(parseLimitGb("-1")).toBeUndefined();
    expect(parseLimitGb("many")).toBeUndefined();
  });
});
