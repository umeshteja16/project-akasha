import React, { useEffect } from "react";
import { useNavigate, useLocation } from "react-router";
import { useAuthStore } from "../../store/auth.store";
import { Loader2 } from "lucide-react";

interface RouteGuardProps {
  children: React.ReactNode;
  requireAuth?: boolean;
}

export function RouteGuard({ children, requireAuth = true }: RouteGuardProps) {
  const { isAuthenticated, isInitialized, setAuth, clearAuth, setInitialized } = useAuthStore();
  const navigate = useNavigate();
  const location = useLocation();

  useEffect(() => {
    async function checkSession() {
      if (isInitialized) return;

      try {
        // Run silent refresh on boot to check for active HTTP-only refresh cookies
        const response = await fetch("/api/v1/auth/refresh", {
          method: "POST",
        });

        if (response.ok) {
          const data = await response.json();
          const token = data.accessToken || data.data?.accessToken;
          if (token) {
            setAuth(token);
          } else {
            clearAuth();
          }
        } else {
          clearAuth();
        }
      } catch (err) {
        clearAuth();
      } finally {
        setInitialized(true);
      }
    }

    checkSession();
  }, [isInitialized, setAuth, clearAuth, setInitialized]);

  useEffect(() => {
    if (!isInitialized) return;

    if (requireAuth && !isAuthenticated) {
      // Redirect unauthenticated users trying to access dashboard to login
      navigate("/login", { replace: true, state: { from: location } });
    } else if (!requireAuth && isAuthenticated) {
      // Redirect authenticated users trying to access login/register to dashboard
      navigate("/dashboard", { replace: true });
    }
  }, [isInitialized, isAuthenticated, requireAuth, navigate, location]);

  // Premium editorial fullscreen loading state during session boots
  if (!isInitialized) {
    return (
      <div className="min-h-screen bg-bg-base flex flex-col items-center justify-center p-6 text-center select-none">
        <div className="inline-flex flex-col items-start gap-1 mb-6 animate-pulse" aria-label="AKASHA Logo">
          <div className="h-[2px] w-[24px] bg-accent-primary rounded-full"></div>
          <div className="h-[2px] w-[18px] bg-accent-primary rounded-full"></div>
          <div className="h-[2px] w-[12px] bg-accent-primary rounded-full"></div>
        </div>
        <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.15em] text-text-muted">
          <Loader2 className="h-4.5 w-4.5 animate-spin text-accent-primary" />
          Initializing Workspace...
        </div>
      </div>
    );
  }

  // Prevent flicker before redirecting unauthenticated users
  if (requireAuth && !isAuthenticated) {
    return null;
  }

  // Prevent flicker before redirecting authenticated users
  if (!requireAuth && isAuthenticated) {
    return null;
  }

  return <>{children}</>;
}
