import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { type ApiToken, claudeCodeCommand, tokenState } from "@/api/tokens";
import { App, createAppDeps } from "@/app";
import { fakeFetch, json, META, USER } from "@/test/fetch";

const TOKEN: ApiToken = {
  id: "11111111-1111-4111-8111-111111111111",
  name: "Old laptop",
  prefix: "akasha_pat_AbCd",
  scopes: ["read"],
  created_at: "2026-10-01T10:00:00Z",
  last_used_at: null,
  expires_at: null,
  revoked_at: null,
};

describe("settings → access tokens", () => {
  it("creates a token, shows it once with setup, and lists tokens", async () => {
    let created: unknown = null;
    const items = [TOKEN];
    const { fetch, calls } = fakeFetch({
      "GET /api/v1/meta": () => json(META),
      "GET /api/v1/me": () => json(USER),
      "GET /api/v1/me/tokens": () => json({ items }),
      "POST /api/v1/me/tokens": async (req) => {
        created = await req.json();
        return json(
          {
            token: { ...TOKEN, id: "2", name: "Desktop", scopes: ["read", "write"] },
            secret: "akasha_pat_SECRET",
          },
          201,
        );
      },
    });
    const history = createMemoryHistory({ initialEntries: ["/settings?tab=tokens"] });
    render(<App deps={createAppDeps({ fetch, baseUrl: "http://akasha.test", history })} />);
    expect(await screen.findByText("Old laptop")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Access tokens", selected: true })).toBeInTheDocument();
    expect(screen.getByText("never used", { exact: false })).toBeInTheDocument();

    const user = userEvent.setup();
    await user.type(screen.getByLabelText("Name"), "Desktop");
    await user.click(screen.getByRole("radio", { name: "Read and write" }));
    await user.click(screen.getByRole("radio", { name: "Never" }));
    await user.click(screen.getByRole("button", { name: "Create token" }));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("akasha_pat_SECRET")).toBeInTheDocument();
    expect(within(dialog).getByText(/claude mcp add --transport http akasha/)).toBeInTheDocument();
    expect(created).toEqual({ name: "Desktop", scopes: ["read", "write"], expires_in_days: null });
    expect(calls).toContain("POST /api/v1/me/tokens");
  });

  it("derives token state and the Claude Code command", () => {
    expect(tokenState(TOKEN)).toBe("active");
    expect(tokenState({ ...TOKEN, revoked_at: "2026-10-02T00:00:00Z" })).toBe("revoked");
    expect(
      tokenState({ ...TOKEN, expires_at: "2026-10-02T00:00:00Z" }, Date.parse("2026-10-03")),
    ).toBe("expired");
    expect(claudeCodeCommand("akasha_pat_x", "https://h.example/")).toBe(
      'claude mcp add --transport http akasha https://h.example/mcp --header "Authorization: Bearer akasha_pat_x"',
    );
  });
});
