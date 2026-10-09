import { describe, expect, it, vi } from "vitest";
import { apiError, fakeFetch, json, USER } from "@/test/fetch";
import { ApiError, createApi, isSessionExpiry, toApiError, unwrap } from "./client";

const BASE = "http://akasha.test";

describe("unwrap", () => {
  it("returns data on success", async () => {
    const { fetch } = fakeFetch({ "GET /api/v1/me": () => json(USER) });
    const api = createApi({ baseUrl: BASE, fetch });
    await expect(unwrap(api.GET("/api/v1/me"))).resolves.toEqual(USER);
  });

  it("maps the server error shape to ApiError", async () => {
    const { fetch } = fakeFetch({
      "POST /api/v1/auth/register": () =>
        apiError(409, "conflict", "an account with this email already exists"),
    });
    const api = createApi({ baseUrl: BASE, fetch });
    const error = await unwrap(
      api.POST("/api/v1/auth/register", { body: { email: "a@b.c", password: "x" } }),
    ).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({
      status: 409,
      code: "conflict",
      message: "an account with this email already exists",
    });
  });

  it("keeps Retry-After on 429", () => {
    const response = new Response("{}", { status: 429, headers: { "retry-after": "6" } });
    const error = toApiError(response, { error: { code: "rate_limited", message: "slow down" } });
    expect(error.retryAfter).toBe(6);
    expect(error.code).toBe("rate_limited");
  });

  it("gives non-JSON errors (proxies) a generic message", async () => {
    const { fetch } = fakeFetch({
      "GET /api/v1/me": () => new Response("<html>Bad gateway</html>", { status: 502 }),
    });
    const api = createApi({ baseUrl: BASE, fetch });
    const error = await unwrap(api.GET("/api/v1/me")).catch((e: unknown) => e);
    expect(error).toMatchObject({ status: 502, code: "http_502" });
    expect((error as ApiError).message).toMatch(/unreachable/);
  });

  it("turns network failures into status 0", async () => {
    const fetch = vi.fn(async () => {
      throw new TypeError("Failed to fetch");
    });
    const api = createApi({ baseUrl: BASE, fetch });
    const error = await unwrap(api.GET("/api/v1/me")).catch((e: unknown) => e);
    expect(error).toMatchObject({ status: 0, code: "network" });
  });
});

describe("session expiry", () => {
  it("calls onUnauthorized for a signed-in request that gets 401", async () => {
    const onUnauthorized = vi.fn();
    const { fetch } = fakeFetch({
      "GET /api/v1/files": () => apiError(401, "unauthorized", "sign in required"),
    });
    const api = createApi({ baseUrl: BASE, fetch, onUnauthorized });
    await unwrap(api.GET("/api/v1/files", { params: { query: {} } })).catch(() => undefined);
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
  });

  it("ignores 401s that mean a wrong password", async () => {
    const onUnauthorized = vi.fn();
    const wrong = () => apiError(401, "unauthorized", "password is incorrect");
    const { fetch } = fakeFetch({
      "POST /api/v1/auth/login": wrong,
      "POST /api/v1/me/password": wrong,
      "DELETE /api/v1/me": wrong,
    });
    const api = createApi({ baseUrl: BASE, fetch, onUnauthorized });
    await unwrap(api.POST("/api/v1/auth/login", { body: { email: "a", password: "b" } })).catch(
      () => undefined,
    );
    await unwrap(
      api.POST("/api/v1/me/password", { body: { current_password: "a", new_password: "b" } }),
    ).catch(() => undefined);
    await unwrap(api.DELETE("/api/v1/me", { body: { password: "a" } })).catch(() => undefined);
    expect(onUnauthorized).not.toHaveBeenCalled();
  });

  it("only 401 counts", () => {
    const request = new Request(`${BASE}/api/v1/files`);
    expect(isSessionExpiry(request, new Response(null, { status: 403 }))).toBe(false);
    expect(isSessionExpiry(request, new Response(null, { status: 401 }))).toBe(true);
  });
});
