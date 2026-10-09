import { createMemoryHistory } from "@tanstack/react-router";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { unwrap } from "@/api/client";
import { apiError, fakeFetch, json, META, USER } from "@/test/fetch";
import { App, createAppDeps } from "./app";

const BASE = "http://akasha.test";
const unauthorized = () => apiError(401, "unauthorized", "sign in required");

function setup(path: string, routes: Parameters<typeof fakeFetch>[0]) {
  const { fetch, calls } = fakeFetch({ "GET /api/v1/meta": () => json(META), ...routes });
  const history = createMemoryHistory({ initialEntries: [path] });
  const deps = createAppDeps({ fetch, baseUrl: BASE, history });
  render(<App deps={deps} />);
  return { deps, calls };
}

describe("auth routing", () => {
  it("sends signed-out visitors to sign-in and remembers where they were going", async () => {
    const { deps } = setup("/settings", { "GET /api/v1/me": unauthorized });
    expect(await screen.findByRole("heading", { name: "Sign in" })).toBeInTheDocument();
    expect(deps.router.state.location.pathname).toBe("/sign-in");
    expect(deps.router.state.location.search).toEqual({ redirect: "/settings" });
  });

  it("signs in and returns to the original page", async () => {
    const user = userEvent.setup();
    let signedIn = false;
    const { deps } = setup("/settings", {
      "GET /api/v1/me": () => (signedIn ? json(USER) : unauthorized()),
      "POST /api/v1/auth/login": () => {
        signedIn = true;
        return json(USER);
      },
    });
    await screen.findByRole("heading", { name: "Sign in" });
    await user.type(screen.getByLabelText("Email"), USER.email);
    await user.type(screen.getByLabelText("Password"), "correct horse battery");
    await user.click(screen.getByRole("button", { name: "Sign in" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
    expect(deps.router.state.location.pathname).toBe("/settings");
  });

  it("shows a friendly message for wrong credentials", async () => {
    const user = userEvent.setup();
    setup("/sign-in", {
      "GET /api/v1/me": unauthorized,
      "POST /api/v1/auth/login": () => apiError(401, "unauthorized", "invalid email or password"),
    });
    await screen.findByRole("heading", { name: "Sign in" });
    await user.type(screen.getByLabelText("Email"), USER.email);
    await user.type(screen.getByLabelText("Password"), "wrong password");
    await user.click(screen.getByRole("button", { name: "Sign in" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("don't match");
  });

  it("keeps signed-in users out of the auth pages", async () => {
    const { deps } = setup("/sign-in", { "GET /api/v1/me": () => json(USER) });
    expect(await screen.findByRole("heading", { level: 1, name: "Library" })).toBeInTheDocument();
    expect(deps.router.state.location.pathname).toBe("/library");
  });

  it("redirects to sign-in when the session ends mid-use", async () => {
    const { deps } = setup("/chat", {
      "GET /api/v1/me": () => json(USER),
      "GET /api/v1/files": unauthorized,
    });
    await screen.findByRole("heading", { level: 1, name: "New conversation" });
    await act(async () => {
      await unwrap(deps.api.GET("/api/v1/files", { params: { query: {} } })).catch(() => undefined);
    });
    await waitFor(() => expect(deps.router.state.location.pathname).toBe("/sign-in"));
    expect(deps.router.state.location.search).toEqual({ redirect: "/chat" });
    expect(deps.queryClient.getQueryData(["me"])).toBeNull();
  });

  it("hides registration when the server has it off", async () => {
    setup("/register", {
      "GET /api/v1/me": unauthorized,
      "GET /api/v1/meta": () => json({ ...META, allow_registration: false }),
    });
    expect(
      await screen.findByRole("heading", { name: "Registration is closed" }),
    ).toBeInTheDocument();
  });
});
