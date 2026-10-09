// Typed API client over the generated OpenAPI schema (schema.d.ts, `just openapi`).
//
// - Same-origin requests with the session cookie (`credentials: "include"`).
// - Every failure becomes an `ApiError` with the server's `{ error: { code, message } }`,
//   or a synthetic code for network failures and non-JSON error bodies.
// - A 401 from a signed-in request means the session ended: `onUnauthorized` runs
//   (the app clears its cache and redirects to sign-in). 401s that only mean "wrong
//   password" (sign-in, password change, account deletion) are excluded.

import createClient, { type Client } from "openapi-fetch";
import type { components, paths } from "./schema";

export type Schemas = components["schemas"];
export type User = Schemas["UserResponse"];
export type ServerMeta = Schemas["ServerMeta"];

/** Error codes the server sends, plus the client-side ones (`network`, `http_<status>`). */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  /** Seconds to wait before retrying (429), when the server says. */
  readonly retryAfter: number | null;

  constructor(status: number, code: string, message: string, retryAfter: number | null = null) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.retryAfter = retryAfter;
  }

  get isUnauthorized(): boolean {
    return this.status === 401;
  }
}

export function isApiError(error: unknown): error is ApiError {
  return error instanceof ApiError;
}

const GENERIC: Record<number, string> = {
  0: "Can't reach the server. Check your connection and try again.",
  413: "That is larger than this server accepts.",
  429: "Too many attempts. Wait a moment and try again.",
  500: "Something went wrong on the server.",
  502: "The server is unreachable right now.",
  503: "The server is temporarily unavailable.",
  504: "The server took too long to answer.",
};

function genericMessage(status: number): string {
  return GENERIC[status] ?? (status >= 500 ? GENERIC[500] : "The request failed.") ?? "";
}

function hasErrorShape(body: unknown): body is Schemas["ErrorBody"] {
  if (typeof body !== "object" || body === null || !("error" in body)) return false;
  const detail = (body as { error: unknown }).error;
  return (
    typeof detail === "object" &&
    detail !== null &&
    typeof (detail as { code?: unknown }).code === "string" &&
    typeof (detail as { message?: unknown }).message === "string"
  );
}

/** Turn an error response (already parsed by openapi-fetch) into an `ApiError`. */
export function toApiError(response: Response, body: unknown): ApiError {
  const retry = Number.parseInt(response.headers.get("retry-after") ?? "", 10);
  const retryAfter = Number.isFinite(retry) ? retry : null;
  if (hasErrorShape(body)) {
    return new ApiError(response.status, body.error.code, body.error.message, retryAfter);
  }
  return new ApiError(
    response.status,
    `http_${response.status}`,
    genericMessage(response.status),
    retryAfter,
  );
}

export function networkError(cause: unknown): ApiError {
  const error = new ApiError(0, "network", genericMessage(0));
  (error as { cause?: unknown }).cause = cause;
  return error;
}

type Result<T> = { data?: T; error?: unknown; response: Response };

/**
 * Await an openapi-fetch call and return its data, throwing `ApiError` on any failure.
 * `const user = await unwrap(api.GET("/api/v1/me"))`
 */
export async function unwrap<T>(call: Promise<Result<T>>): Promise<T> {
  let result: Result<T>;
  try {
    result = await call;
  } catch (cause) {
    if (cause instanceof ApiError) throw cause;
    throw networkError(cause);
  }
  if (!result.response.ok) throw toApiError(result.response, result.error);
  return result.data as T;
}

/** Requests whose 401 means "wrong credentials", not "signed out". */
const CREDENTIAL_CHECKS: ReadonlyArray<[method: string, path: string]> = [
  ["POST", "/api/v1/auth/login"],
  ["POST", "/api/v1/me/password"],
  ["DELETE", "/api/v1/me"],
];

export function isSessionExpiry(request: Request, response: Response): boolean {
  if (response.status !== 401) return false;
  const path = new URL(request.url).pathname;
  return !CREDENTIAL_CHECKS.some(([m, p]) => m === request.method && p === path);
}

export type Api = Client<paths>;

export interface ApiOptions {
  /** Defaults to the page origin (tests pass an absolute one). */
  baseUrl?: string;
  /** Inject a fetch for tests. */
  fetch?: typeof globalThis.fetch;
  /** Called when a signed-in request comes back 401 (session ended). */
  onUnauthorized?: () => void;
}

export function createApi(options: ApiOptions = {}): Api {
  const baseUrl = options.baseUrl ?? (typeof window === "undefined" ? "" : window.location.origin);
  const client = createClient<paths>({
    baseUrl,
    credentials: "include",
    ...(options.fetch ? { fetch: options.fetch } : {}),
  });
  client.use({
    onResponse({ request, response }) {
      if (isSessionExpiry(request, response)) options.onUnauthorized?.();
      return undefined;
    },
  });
  return client;
}
