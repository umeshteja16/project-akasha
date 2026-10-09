import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getRouteApi, Link, useRouter } from "@tanstack/react-router";
import { ArrowRightIcon, LockIcon } from "lucide-react";
import { type FormEvent, useState } from "react";
import { isApiError, unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { metaQuery } from "@/api/queries";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/skeleton";
import { safeRedirect, sentence, signedIn } from "@/lib/session";
import { useDocumentTitle } from "@/lib/use-document-title";
import { FormError } from "./form-error";

const route = getRouteApi("/auth/register");

/** Must match the server (crates/app/src/auth/password.rs). */
export const PASSWORD_MIN = 8;

function registerErrorMessage(error: unknown): string {
  if (!isApiError(error)) return "Something went wrong. Try again.";
  if (error.status === 409) return "An account with this email already exists. Sign in instead?";
  if (error.status === 403) return "This server isn't accepting new accounts.";
  if (error.status === 429) return "Too many attempts. Wait a minute, then try again.";
  return sentence(error.message);
}

export function RegisterPage() {
  useDocumentTitle("Create your account");
  const api = useApi();
  const queryClient = useQueryClient();
  const router = useRouter();
  const { redirect } = route.useSearch();
  const meta = useQuery(metaQuery(api));
  const [displayName, setDisplayName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [touched, setTouched] = useState(false);

  const register = useMutation({
    mutationFn: () =>
      unwrap(
        api.POST("/api/v1/auth/register", {
          body: { email, password, display_name: displayName.trim() || null },
        }),
      ),
    onSuccess: async (user) => {
      signedIn(queryClient, user);
      await router.navigate({ href: safeRedirect(redirect), replace: true });
    },
  });

  const tooShort = password.length > 0 && password.length < PASSWORD_MIN;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setTouched(true);
    if (password.length < PASSWORD_MIN) return;
    register.mutate();
  };

  if (meta.isPending) {
    return (
      <div className="grid gap-4" aria-busy="true">
        <Skeleton className="h-9 w-2/3" />
        <Skeleton className="h-10" />
        <Skeleton className="h-10" />
      </div>
    );
  }

  if (meta.data && !meta.data.allow_registration) {
    return (
      <div className="grid gap-5">
        <span className="grid size-11 place-items-center rounded-full border border-border bg-surface text-fg-subtle">
          <LockIcon className="size-5" strokeWidth={1.6} />
        </span>
        <h1 className="display text-3xl text-fg">Registration is closed</h1>
        <p className="text-sm text-fg-muted">
          This Akasha server isn't accepting new accounts. Ask its owner to invite you, or sign in
          if you already have an account.
        </p>
        <Button asChild variant="secondary" size="lg">
          <Link to="/sign-in">Go to sign in</Link>
        </Button>
      </div>
    );
  }

  return (
    <div className="grid gap-8">
      <div className="grid gap-2">
        <p className="eyebrow">New account</p>
        <h1 className="display text-3xl text-fg">Start your library</h1>
        <p className="text-sm text-fg-muted">Everything stays on this server.</p>
      </div>
      <form className="grid gap-5" onSubmit={submit}>
        <FormError message={register.isError ? registerErrorMessage(register.error) : null} />
        <Field
          label="Name"
          name="name"
          autoComplete="name"
          hint="Optional. Shown only to you."
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          maxLength={100}
        />
        <Field
          label="Email"
          type="email"
          name="email"
          autoComplete="email"
          required
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
        <Field
          label="Password"
          type="password"
          name="new-password"
          autoComplete="new-password"
          required
          minLength={PASSWORD_MIN}
          maxLength={128}
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          onBlur={() => setTouched(true)}
          hint={`At least ${PASSWORD_MIN} characters. A short sentence works well.`}
          error={touched && tooShort ? `Use at least ${PASSWORD_MIN} characters.` : null}
        />
        <Button type="submit" size="lg" disabled={register.isPending} className="mt-1 w-full">
          {register.isPending ? "Creating account…" : "Create account"}
          {register.isPending ? null : <ArrowRightIcon />}
        </Button>
      </form>
      <p className="text-sm text-fg-muted">
        Already have an account?{" "}
        <Link
          to="/sign-in"
          search={redirect ? { redirect } : {}}
          className="font-medium text-accent-text underline-offset-4 hover:underline"
        >
          Sign in
        </Link>
      </p>
    </div>
  );
}
