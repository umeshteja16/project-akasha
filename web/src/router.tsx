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
import { validateFileSearch } from "@/features/files/passage";
import { validateSearchParams } from "@/features/search/search-params";
import { safeRedirect } from "@/lib/session";
import { validateActivitySearch } from "@/routes/activity/activity-search";
import { AuthLayout } from "@/routes/auth/auth-layout";
import { validateChatSearch } from "@/routes/chat/chat-search";
import { ErrorPage } from "@/routes/error-page";
import { validateLibrarySearch } from "@/routes/library/library-search";
import { NotFoundPage } from "@/routes/not-found";
import { RootLayout } from "@/routes/root";
import { RouteError } from "@/routes/route-error";
import { validateSettingsSearch } from "@/routes/settings/settings-search";

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
  errorComponent: ErrorPage,
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
  errorComponent: ErrorPage,
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
  validateSearch: validateFileSearch,
  component: lazyRouteComponent(() => import("@/routes/file/file-page"), "FilePage"),
});

const searchRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/search",
  validateSearch: validateSearchParams,
  component: lazyRouteComponent(() => import("@/routes/search/search-page"), "SearchPage"),
});

const chatRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/chat",
  validateSearch: validateChatSearch,
  component: lazyRouteComponent(() => import("@/routes/chat/chat-layout"), "ChatLayout"),
});

const chatIndexRoute = createRoute({
  getParentRoute: () => chatRoute,
  path: "/",
  component: lazyRouteComponent(() => import("@/routes/chat/chat-home"), "ChatHome"),
});

const conversationRoute = createRoute({
  getParentRoute: () => chatRoute,
  path: "$conversationId",
  component: lazyRouteComponent(
    () => import("@/routes/chat/conversation-page"),
    "ConversationPage",
  ),
});

const collectionsRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/collections",
  component: lazyRouteComponent(
    () => import("@/routes/collections/collections-page"),
    "CollectionsPage",
  ),
});

const collectionRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/collections/$collectionId",
  component: lazyRouteComponent(
    () => import("@/routes/collections/collection-page"),
    "CollectionPage",
  ),
});

const activityRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/activity",
  validateSearch: validateActivitySearch,
  component: lazyRouteComponent(() => import("@/routes/activity/activity-page"), "ActivityPage"),
});

const settingsRoute = createRoute({
  getParentRoute: () => appLayout,
  path: "/settings",
  validateSearch: validateSettingsSearch,
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
    chatRoute.addChildren([chatIndexRoute, conversationRoute]),
    collectionsRoute,
    collectionRoute,
    activityRoute,
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
    // Each screen gets its own boundary, inside the shell (layouts use ErrorPage).
    defaultErrorComponent: RouteError,
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
