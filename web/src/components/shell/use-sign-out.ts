import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useRouter } from "@tanstack/react-router";
import { unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { toast } from "@/components/ui/toast";
import { signedOut } from "@/lib/session";

export function useSignOut() {
  const api = useApi();
  const queryClient = useQueryClient();
  const router = useRouter();
  return useMutation({
    mutationFn: () => unwrap(api.POST("/api/v1/auth/logout")),
    onSettled: async (_data, error) => {
      // Even if the request failed, leave the signed-in UI: the cookie is the
      // server's business, and a retry from the sign-in page is harmless.
      signedOut(queryClient);
      await router.navigate({ to: "/sign-in", replace: true });
      if (error) toast({ title: "Signed out on this device", tone: "info" });
    },
  });
}
