// Route tree (code-based, fully typed). Two pathless layouts:
// - "auth": sign-in and register; signed-in users are sent on to the app.
// - "app": everything else; requires a session, renders the shell.

import type { QueryClient } from "@tanstack/react-query";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  lazyRouteComponent,
  redirect,
} from "@tanstack/react-router";
import type { Api } from "@/api/client";
import { meQuery } from "@/api/queries";
import { AppShell } from "@/components/shell/app-shell";
import { safeRedirect } from "@/lib/session";
import { AuthLayout } from "@/routes/auth/auth-layout";
import { ErrorPage } from "@/routes/error-page";
import { validateLibrarySearch } from "@/routes/library/library-search";
import { NotFoundPage } from "@/routes/not-found";
import { RootLayout } from "@/routes/root";

export interface RouterContext {
  queryClient: QueryClient;
  api: Api;
}

const rootRoute = createRootRouteWithContext<RouterContext>()({
  component: RootLayout,
  notFoundComponent: NotFoundPage,
  errorComponent: ErrorPage,
});

const authLayout = createRoute({
  getParentRoute: () => rootRoute,
  id: "auth",
  component: AuthLayout,
  beforeLoad: async ({ context, search }) => {
    const me = await context.queryClient.ensureQueryData(meQuery(context.api));
    if (me) {
      const target = (search as { redirect?: string }).redirect;
      throw redirect({ href: safeRedirect(target), replace: true });
    }
  },
});

const validateRedirect = (search: Record<string, unknown>): { redirect?: string } =>
  typeof search.redirect === "string" ? { redirect: search.redirect } : {};

const signInRoute = createRoute({
  getParentRoute: () => authLayout,
  path: "/sign-in",
  validateSearch: validateRedirect,
  component: lazyRouteComponent(() => import("@/routes/auth/sign-in"), "SignInPage"),
});

const registerRoute = createRoute({
  getParentRoute: () => authLayout,
  path: "/register",
  validateSearch: validateRedirect,
  component: lazyRouteComponent(() => import("@/routes/auth/register"), "RegisterPage"),
});

const appLayout = createRoute({
  getParentRoute: () => rootRoute,
  id: "app",
  component: AppShell,
  beforeLoad: async ({ context, location }) => {
    const me = await context.queryClient.ensureQueryData(meQuery(context.api));
    if (!me) {
      const back = location.href === "/" ? undefined : location.href;
      throw redirect({ to: "/sign-in", search: back ? { redirect: back } : {}, replace: true });
    }
    return { me };
  },
});

const indexRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/library", replace: true });
  },
});

const libraryRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/library",
  validateSearch: validateLibrarySearch,
  component: lazyRouteComponent(() => import("@/routes/library/library-page"), "LibraryPage"),
});

const fileRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/files/$fileId",
  component: lazyRouteComponent(() => import("@/routes/file/file-page"), "FilePage"),
});

const searchRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/search",
  validateSearch: (search: Record<string, unknown>): { q?: string } =>
    typeof search.q === "string" && search.q.trim() ? { q: search.q.trim() } : {},
  component: lazyRouteComponent(() => import("@/routes/search"), "SearchPage"),
});

const chatRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/chat",
  component: lazyRouteComponent(() => import("@/routes/chat"), "ChatPage"),
});

const settingsRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/settings",
  component: lazyRouteComponent(() => import("@/routes/settings/settings-page"), "SettingsPage"),
});

const designRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/design",
  component: lazyRouteComponent(() => import("@/routes/design"), "DesignPage"),
});

const routeTree = rootRoute.addChildren([
  authLayout.addChildren([signInRoute, registerRoute]),
  appLayout.addChildren([
    indexRoute,
    libraryRoute,
    fileRoute,
    searchRoute,
    chatRoute,
    settingsRoute,
    designRoute,
  ]),
]);

export function createAppRouter(
  context: RouterContext,
  history?: Parameters<typeof createRouter>[0]["history"],
) {
  return createRouter({
    routeTree,
    context,
    history,
    defaultPreload: "intent",
    // Loaders read through TanStack Query, which does its own caching.
    defaultPreloadStaleTime: 0,
    scrollRestoration: true,
  });
}

export type AppRouter = ReturnType<typeof createAppRouter>;

declare module "@tanstack/react-router" {
  interface Register {
    router: AppRouter;
  }
}
