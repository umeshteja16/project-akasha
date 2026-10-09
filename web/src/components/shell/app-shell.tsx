import { Outlet, useRouterState } from "@tanstack/react-router";
import { UploadProvider } from "@/features/upload/upload-context";
import { UploadPanel } from "@/features/upload/upload-panel";
import { cn } from "@/lib/utils";
import { CommandPaletteProvider } from "./command-palette";
import { MobileHeader, MobileTabBar } from "./mobile-nav";
import { Sidebar } from "./sidebar";
import { UploadDropZone } from "./upload-drop-zone";
import { useGlobalShortcuts } from "./use-global-shortcuts";

/** The signed-in frame: sidebar (desktop) or top bar + tab bar (phone). */
export function AppShell() {
  return (
    <UploadProvider>
      <CommandPaletteProvider>
        <Shell />
      </CommandPaletteProvider>
    </UploadProvider>
  );
}

/** Screens that manage their own height and scrolling (chat): no page padding. */
function useFullBleed(): boolean {
  return useRouterState({
    select: (s) => s.location.pathname === "/chat" || s.location.pathname.startsWith("/chat/"),
  });
}

function Shell() {
  useGlobalShortcuts();
  const fullBleed = useFullBleed();
  return (
    <>
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-[70] focus:rounded-md focus:bg-surface focus:px-3 focus:py-2 focus:shadow-md"
      >
        Skip to content
      </a>
      <div className="min-h-dvh bg-bg md:grid md:grid-cols-[var(--sidebar-width)_minmax(0,1fr)]">
        <Sidebar />
        <div className="flex min-w-0 flex-col">
          <MobileHeader />
          <main
            id="main"
            tabIndex={-1}
            className={cn(
              "flex-1 outline-none",
              fullBleed ? "min-h-0" : "px-4 pt-6 pb-28 sm:px-8 md:pt-12 md:pb-16 lg:px-12",
            )}
          >
            {fullBleed ? (
              <Outlet />
            ) : (
              <div className="mx-auto w-full max-w-[var(--content-max)] animate-fade-in">
                <Outlet />
              </div>
            )}
          </main>
        </div>
        <MobileTabBar />
      </div>
      <UploadDropZone />
      <UploadPanel />
    </>
  );
}
