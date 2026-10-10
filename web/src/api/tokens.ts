// Personal API tokens for MCP clients and scripts (`/api/v1/me/tokens`).

import { queryOptions } from "@tanstack/react-query";
import { type Api, type Schemas, unwrap } from "./client";

export type ApiToken = Schemas["TokenResponse"];
export type CreatedToken = Schemas["CreatedToken"];
export type TokenScope = Schemas["TokenScope"];

export const tokenKeys = { all: ["me", "tokens"] as const };

export const tokensQuery = (api: Api) =>
  queryOptions({
    queryKey: tokenKeys.all,
    queryFn: ({ signal }) => unwrap(api.GET("/api/v1/me/tokens", { signal })),
  });

export type TokenState = "active" | "expired" | "revoked";

export function tokenState(token: ApiToken, now: number = Date.now()): TokenState {
  if (token.revoked_at) return "revoked";
  if (token.expires_at && Date.parse(token.expires_at) <= now) return "expired";
  return "active";
}

/** The `/mcp` endpoint of the server the UI was loaded from. */
export function mcpEndpoint(origin: string = window.location.origin): string {
  return `${origin.replace(/\/$/, "")}/mcp`;
}

/** Copy-paste command for Claude Code. */
export function claudeCodeCommand(secret: string, origin?: string): string {
  return `claude mcp add --transport http akasha ${mcpEndpoint(origin)} --header "Authorization: Bearer ${secret}"`;
}
