import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { type User, unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { keys } from "@/api/queries";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { toast } from "@/components/ui/toast";
import { sentence } from "@/lib/session";
import { FormError } from "@/routes/auth/form-error";
import { SettingsSection } from "./settings-section";

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "long" });

export function ProfileSection({ me }: { me: User }) {
  const api = useApi();
  const queryClient = useQueryClient();
  const [name, setName] = useState(me.display_name ?? "");
  const save = useMutation({
    mutationFn: () =>
      unwrap(api.PATCH("/api/v1/me", { body: { display_name: name.trim() || null } })),
    onSuccess: (user) => {
      queryClient.setQueryData(keys.me, user);
      setName(user.display_name ?? "");
      toast({ title: "Profile saved", tone: "success" });
    },
  });
  const dirty = (me.display_name ?? "") !== name.trim();

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (dirty) save.mutate();
  };

  return (
    <SettingsSection
      id="profile"
      title="Profile"
      description="How Akasha greets you. Your email is how you sign in."
    >
      <Card>
        <form onSubmit={submit}>
          <CardContent className="grid gap-5">
            <FormError message={save.isError ? sentence(save.error.message) : null} />
            <Field
              label="Display name"
              name="display_name"
              autoComplete="name"
              maxLength={100}
              value={name}
              onChange={(e) => setName(e.target.value)}
              hint="Leave empty to use your email."
            />
            <dl className="grid gap-4 text-sm sm:grid-cols-2">
              <div className="grid gap-1">
                <dt className="eyebrow">Email</dt>
                <dd className="truncate text-fg">{me.email}</dd>
              </div>
              <div className="grid gap-1">
                <dt className="eyebrow">Member since</dt>
                <dd className="text-fg">{dateFormat.format(new Date(me.created_at))}</dd>
              </div>
            </dl>
          </CardContent>
          <CardFooter>
            <Button type="submit" disabled={!dirty || save.isPending}>
              {save.isPending ? "Saving…" : "Save profile"}
            </Button>
          </CardFooter>
        </form>
      </Card>
    </SettingsSection>
  );
}
