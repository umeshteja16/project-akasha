import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/api";
import { useAuthStore } from "../store/auth.store";

export interface User {
  id: string;
  email: string;
  displayName?: string | null;
}

export function useUser() {
  const accessToken = useAuthStore((state) => state.accessToken);
  const isAuthenticated = useAuthStore((state) => state.isAuthenticated);

  return useQuery<User, Error>({
    queryKey: ["user", accessToken],
    queryFn: async () => {
      const response = await api.get("/api/v1/me");
      // Since response format might wrap in data: { data: { id, email } } or { id, email }
      return response.data || response;
    },
    enabled: isAuthenticated && !!accessToken,
    staleTime: 5 * 60 * 1000, // 5 minutes
    retry: false,
  });
}
