import { useMutation } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { isApiError, unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { toast } from "@/components/ui/toast";
import { sentence } from "@/lib/session";
import { FormError } from "@/routes/auth/form-error";
import { PASSWORD_MIN } from "@/routes/auth/register";
import { SettingsSection } from "./settings-section";

function message(error: unknown): string {
  if (!isApiError(error)) return "Something went wrong. Try again.";
  if (error.status === 401) return "Your current password is incorrect.";
  if (error.status === 429) return "Too many attempts. Wait a minute, then try again.";
  return sentence(error.message);
}

export function PasswordSection() {
  const api = useApi();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [localError, setLocalError] = useState<string | null>(null);

  const change = useMutation({
    mutationFn: () =>
      unwrap(
        api.POST("/api/v1/me/password", {
          body: { current_password: current, new_password: next },
        }),
      ),
    onSuccess: () => {
      setCurrent("");
      setNext("");
      setConfirm("");
      toast({
        title: "Password changed",
        description: "Other devices have been signed out.",
        tone: "success",
      });
    },
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (next.length < PASSWORD_MIN) {
      setLocalError(`Use at least ${PASSWORD_MIN} characters for the new password.`);
      return;
    }
    if (next !== confirm) {
      setLocalError("The new passwords don't match.");
      return;
    }
    setLocalError(null);
    change.mutate();
  };

  const error = localError ?? (change.isError ? message(change.error) : null);
  return (
    <SettingsSection
      id="password"
      title="Password"
      description="Changing it signs out every other device."
    >
      <Card>
        <form onSubmit={submit}>
          <CardContent className="grid gap-5">
            <FormError message={error} />
            <Field
              label="Current password"
              type="password"
              autoComplete="current-password"
              required
              value={current}
              onChange={(e) => setCurrent(e.target.value)}
            />
            <div className="grid gap-5 sm:grid-cols-2">
              <Field
                label="New password"
                type="password"
                autoComplete="new-password"
                required
                minLength={PASSWORD_MIN}
                maxLength={128}
                value={next}
                onChange={(e) => setNext(e.target.value)}
              />
              <Field
                label="Repeat new password"
                type="password"
                autoComplete="new-password"
                required
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
              />
            </div>
          </CardContent>
          <CardFooter>
            <Button
              type="submit"
              variant="secondary"
              disabled={change.isPending || !current || !next}
            >
              {change.isPending ? "Changing…" : "Change password"}
            </Button>
          </CardFooter>
        </form>
      </Card>
    </SettingsSection>
  );
}
