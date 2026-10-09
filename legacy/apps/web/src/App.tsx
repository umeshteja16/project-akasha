import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter, Routes, Route } from "react-router";
import { LoginPage } from "./pages/auth/LoginPage";
import { RegisterPage } from "./pages/auth/RegisterPage";
import { LandingPage } from "./pages/landing/LandingPage";
import { DashboardPage } from "./pages/dashboard/DashboardPage";
import { SearchPage } from "./pages/search/SearchPage";
import { SettingsPage } from "./pages/settings/SettingsPage";
import { ActivityTimelinePage } from "./pages/activity/ActivityTimelinePage";
import { CollectionPage } from "./pages/collections/CollectionPage";
import { ChatPage } from "./pages/chat/ChatPage";
import { ProfilePage } from "./pages/profile/ProfilePage";
import { NotFoundPage } from "./pages/error/NotFoundPage";
import { RouteGuard } from "./components/auth/RouteGuard";
import { AppLayout } from "./components/layout/AppLayout";
import { Toaster } from "sonner";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          {/* Landing Page */}
          <Route
            path="/"
            element={<LandingPage />}
          />

          {/* Public Auth Routes (RouteGuard maps auto-redirects to dashboard if already authenticated) */}
          <Route
            path="/login"
            element={
              <RouteGuard requireAuth={false}>
                <LoginPage />
              </RouteGuard>
            }
          />
          <Route
            path="/register"
            element={
              <RouteGuard requireAuth={false}>
                <RegisterPage />
              </RouteGuard>
            }
          />

          {/* Protected Main Workspace Routes */}
          <Route
            path="/dashboard"
            element={
              <RouteGuard requireAuth={true}>
                <AppLayout>
                  <DashboardPage />
                </AppLayout>
              </RouteGuard>
            }
          />
          <Route
            path="/search"
            element={
              <RouteGuard requireAuth={true}>
                <AppLayout>
                  <SearchPage />
                </AppLayout>
              </RouteGuard>
            }
          />
          <Route
            path="/chat"
            element={
              <RouteGuard requireAuth={true}>
                <AppLayout>
                  <ChatPage />
                </AppLayout>
              </RouteGuard>
            }
          />
          <Route
            path="/collections/:id"
            element={
              <RouteGuard requireAuth={true}>
                <AppLayout>
                  <CollectionPage />
                </AppLayout>
              </RouteGuard>
            }
          />

          <Route
            path="/settings"
            element={
              <RouteGuard requireAuth={true}>
                <AppLayout>
                  <SettingsPage />
                </AppLayout>
              </RouteGuard>
            }
          />

          <Route
            path="/activity"
            element={
              <RouteGuard requireAuth={true}>
                <AppLayout>
                  <ActivityTimelinePage />
                </AppLayout>
              </RouteGuard>
            }
          />
          <Route
            path="/profile"
            element={
              <RouteGuard requireAuth={true}>
                <AppLayout>
                  <ProfilePage />
                </AppLayout>
              </RouteGuard>
            }
          />

          {/* Fallback Redirects */}
          <Route path="*" element={<NotFoundPage />} />
        </Routes>
      </BrowserRouter>
      <Toaster position="bottom-right" closeButton />
    </QueryClientProvider>
  );
}
