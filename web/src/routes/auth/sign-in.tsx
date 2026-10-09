import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getRouteApi, Link, useRouter } from "@tanstack/react-router";
import { ArrowRightIcon } from "lucide-react";
import { type FormEvent, useState } from "react";
import { isApiError, unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { metaQuery } from "@/api/queries";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { safeRedirect, sentence, signedIn } from "@/lib/session";
import { FormError } from "./form-error";

const route = getRouteApi("/auth/sign-in");

export function signInErrorMessage(error: unknown): string {
  if (!isApiError(error)) return "Something went wrong. Try again.";
  if (error.status === 401) return "That email and password don't match an account.";
  if (error.status === 429) return "Too many attempts. Wait a minute, then try again.";
  return sentence(error.message);
}

export function SignInPage() {
  const api = useApi();
  const queryClient = useQueryClient();
  const router = useRouter();
  const { redirect } = route.useSearch();
  const meta = useQuery(metaQuery(api));
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");

  const signIn = useMutation({
    mutationFn: () => unwrap(api.POST("/api/v1/auth/login", { body: { email, password } })),
    onSuccess: async (user) => {
      signedIn(queryClient, user);
      await router.navigate({ href: safeRedirect(redirect), replace: true });
    },
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    signIn.mutate();
  };

  return (
    <div className="grid gap-8">
      <div className="grid gap-2">
        <p className="eyebrow">Welcome back</p>
        <h1 className="display text-3xl text-fg">Sign in</h1>
        <p className="text-sm text-fg-muted">Pick up where you left off.</p>
      </div>
      <form className="grid gap-5" onSubmit={submit}>
        <FormError message={signIn.isError ? signInErrorMessage(signIn.error) : null} />
        <Field
          label="Email"
          type="email"
          name="email"
          autoComplete="username"
          required
          autoFocus
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
        <Field
          label="Password"
          type="password"
          name="password"
          autoComplete="current-password"
          required
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <Button type="submit" size="lg" disabled={signIn.isPending} className="mt-1 w-full">
          {signIn.isPending ? "Signing in…" : "Sign in"}
          {signIn.isPending ? null : <ArrowRightIcon />}
        </Button>
      </form>
      {meta.data?.allow_registration ? (
        <p className="text-sm text-fg-muted">
          New here?{" "}
          <Link
            to="/register"
            search={redirect ? { redirect } : {}}
            className="font-medium text-accent-text underline-offset-4 hover:underline"
          >
            Create an account
          </Link>
        </p>
      ) : null}
    </div>
  );
}
