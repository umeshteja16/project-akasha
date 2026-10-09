// Composition root: query cache, API client, router and the global providers.

import { QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type RouterHistory, RouterProvider } from "@tanstack/react-router";
import { type Api, createApi, isApiError } from "@/api/client";
import { ApiProvider } from "@/api/context";
import { Toaster } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import { isSignedIn, signedOut } from "@/lib/session";
import { ThemeProvider } from "@/lib/theme";
import { type AppRouter, createAppRouter } from "@/router";

export interface AppDeps {
  queryClient: QueryClient;
  api: Api;
  router: AppRouter;
}

/** Don't retry what cannot succeed: client errors (4xx) and signed-out requests. */
function shouldRetry(failures: number, error: unknown): boolean {
  if (isApiError(error) && error.status >= 400 && error.status < 500) return false;
  return failures < 2;
}

export function createAppDeps(
  options: { fetch?: typeof fetch; baseUrl?: string; history?: RouterHistory } = {},
): AppDeps {
  const queryClient = new QueryClient({
    queryCache: new QueryCache(),
    defaultOptions: {
      queries: { retry: shouldRetry, staleTime: 30_000 },
      mutations: { retry: false },
    },
  });
  // The router needs the client and the client's 401 handler needs the router.
  let router: AppRouter | null = null;
  const api = createApi({
    ...(options.fetch ? { fetch: options.fetch } : {}),
    ...(options.baseUrl ? { baseUrl: options.baseUrl } : {}),
    onUnauthorized: () => {
      // Only a session that existed can end; the sign-in bootstrap 401 is expected.
      if (!router || !isSignedIn(queryClient)) return;
      signedOut(queryClient);
      const here = router.state.location.href;
      void router.navigate({ to: "/sign-in", search: { redirect: here }, replace: true });
    },
  });
  router = createAppRouter({ queryClient, api }, options.history);
  return { queryClient, api, router };
}

export function App({ deps }: { deps: AppDeps }) {
  return (
    <ThemeProvider>
      <QueryClientProvider client={deps.queryClient}>
        <ApiProvider api={deps.api}>
          <TooltipProvider delayDuration={300}>
            <RouterProvider router={deps.router} />
            <Toaster />
          </TooltipProvider>
        </ApiProvider>
      </QueryClientProvider>
    </ThemeProvider>
  );
}
