import { create } from "zustand";

interface AuthState {
  accessToken: string | null;
  isAuthenticated: boolean;
  isInitialized: boolean;
  setAuth: (accessToken: string) => void;
  clearAuth: () => void;
  setInitialized: (initialized: boolean) => void;
}

export const useAuthStore = create<AuthState>((set) => ({
  accessToken: null,
  isAuthenticated: false,
  isInitialized: false,
  setAuth: (accessToken) =>
    set({
      accessToken,
      isAuthenticated: true,
    }),
  clearAuth: () =>
    set({
      accessToken: null,
      isAuthenticated: false,
    }),
  setInitialized: (isInitialized) => set({ isInitialized }),
}));

