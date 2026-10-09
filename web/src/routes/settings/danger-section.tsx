import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useRouter } from "@tanstack/react-router";
import { TriangleAlertIcon } from "lucide-react";
import { type FormEvent, useState } from "react";
import { isApiError, unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { toast } from "@/components/ui/toast";
import { sentence, signedOut } from "@/lib/session";
import { FormError } from "@/routes/auth/form-error";
import { SettingsSection } from "./settings-section";

function message(error: unknown): string {
  if (!isApiError(error)) return "Something went wrong. Try again.";
  if (error.status === 401) return "That password is incorrect.";
  if (error.status === 429) return "Too many attempts. Wait a minute, then try again.";
  return sentence(error.message);
}

export function DangerSection({ email }: { email: string }) {
  const api = useApi();
  const queryClient = useQueryClient();
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [password, setPassword] = useState("");
  const [confirmText, setConfirmText] = useState("");

  const remove = useMutation({
    mutationFn: () => unwrap(api.DELETE("/api/v1/me", { body: { password } })),
    onSuccess: async () => {
      setOpen(false);
      signedOut(queryClient);
      await router.navigate({ to: "/sign-in", replace: true });
      toast({ title: "Your account has been deleted", description: "Everything in it is gone." });
    },
  });

  const confirmed = confirmText.trim().toLowerCase() === "delete";
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (confirmed && password) remove.mutate();
  };

  return (
    <SettingsSection
      id="danger"
      title="Delete account"
      description="Removes your account, every file, note and conversation. This can't be undone."
    >
      <Card className="border-danger/30">
        <CardContent className="flex flex-wrap items-center justify-between gap-4">
          <p className="flex items-center gap-2 text-sm text-fg-muted">
            <TriangleAlertIcon className="size-4 text-danger" aria-hidden />
            Download anything you want to keep first.
          </p>
          <Dialog
            open={open}
            onOpenChange={(next) => {
              setOpen(next);
              if (!next) {
                setPassword("");
                setConfirmText("");
                remove.reset();
              }
            }}
          >
            <DialogTrigger asChild>
              <Button variant="danger">Delete account…</Button>
            </DialogTrigger>
            <DialogContent>
              <form onSubmit={submit} className="grid gap-5">
                <DialogHeader>
                  <DialogTitle>Delete your account?</DialogTitle>
                  <DialogDescription>
                    <span className="font-medium text-fg">{email}</span> and everything it owns will
                    be permanently deleted.
                  </DialogDescription>
                </DialogHeader>
                <FormError message={remove.isError ? message(remove.error) : null} />
                <Field
                  label="Password"
                  type="password"
                  autoComplete="current-password"
                  required
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                />
                <Field
                  label='Type "delete" to confirm'
                  autoComplete="off"
                  spellCheck={false}
                  required
                  value={confirmText}
                  onChange={(e) => setConfirmText(e.target.value)}
                />
                <DialogFooter>
                  <DialogClose asChild>
                    <Button variant="secondary">Keep my account</Button>
                  </DialogClose>
                  <Button
                    type="submit"
                    variant="danger"
                    disabled={!confirmed || !password || remove.isPending}
                  >
                    {remove.isPending ? "Deleting…" : "Delete forever"}
                  </Button>
                </DialogFooter>
              </form>
            </DialogContent>
          </Dialog>
        </CardContent>
      </Card>
    </SettingsSection>
  );
}
