// Session transitions shared by the router, the API client and components.

import type { QueryClient } from "@tanstack/react-query";
import type { User } from "@/api/client";
import { keys } from "@/api/queries";

/** Paths we may send the user back to after signing in: same-origin app paths only. */
export function safeRedirect(target: string | undefined): string {
  if (!target?.startsWith("/") || target.startsWith("//") || target.startsWith("/\\")) {
    return "/library";
  }
  if (target.startsWith("/sign-in") || target.startsWith("/register")) return "/library";
  return target;
}

/** Signed in: remember the user (the app layout guard reads it). */
export function signedIn(queryClient: QueryClient, user: User) {
  queryClient.setQueryData(keys.me, user);
}

/** Signed out (or session ended): forget the user and every cached query of theirs. */
export function signedOut(queryClient: QueryClient) {
  queryClient.setQueryData(keys.me, null);
  queryClient.removeQueries({ predicate: (query) => query.queryKey[0] !== keys.me[0] });
}

/** Whether this tab currently believes someone is signed in. */
export function isSignedIn(queryClient: QueryClient): boolean {
  return Boolean(queryClient.getQueryData(keys.me));
}

/** Sentence-case a server message ("password is incorrect" → "Password is incorrect."). */
export function sentence(message: string): string {
  const trimmed = message.trim();
  if (!trimmed) return trimmed;
  const capital = trimmed.charAt(0).toUpperCase() + trimmed.slice(1);
  return /[.!?]$/.test(capital) ? capital : `${capital}.`;
}
