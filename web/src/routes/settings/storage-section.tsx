import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { type User, unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { keys } from "@/api/queries";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Progress } from "@/components/ui/progress";
import { toast } from "@/components/ui/toast";
import { formatBytes } from "@/lib/format";
import { sentence } from "@/lib/session";
import { FormError } from "@/routes/auth/form-error";
import { SettingsSection } from "./settings-section";

/** The box is in GB, binary like `formatBytes`. */
const GB = 1024 ** 3;

/** Bytes → what the box shows; empty when there is no limit. */
export function toLimitGb(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "";
  const gb = bytes / GB;
  return String(gb >= 10 ? Math.round(gb) : Number(gb.toFixed(2)));
}

/** The box → bytes. `null`: no limit. `undefined`: not a usable number. */
export function parseLimitGb(raw: string): number | null | undefined {
  const text = raw.trim();
  if (!text) return null;
  const gb = Number(text);
  if (!Number.isFinite(gb) || gb < 0) return undefined;
  return Math.round(gb * GB);
}

export function StorageSection({ me }: { me: User }) {
  const api = useApi();
  const queryClient = useQueryClient();
  const used = me.storage.used_bytes;
  const quota = me.storage.quota_bytes ?? null;
  const remaining = me.storage.remaining_bytes ?? null;
  const [limit, setLimit] = useState(() => toLimitGb(quota));
  const parsed = parseLimitGb(limit);
  const dirty = parsed !== undefined && parsed !== quota;
  const over = quota !== null && used > quota;

  const save = useMutation({
    mutationFn: (bytes: number | null) =>
      unwrap(api.PATCH("/api/v1/me", { body: { storage_quota_bytes: bytes } })),
    onSuccess: (user) => {
      queryClient.setQueryData(keys.me, user);
      setLimit(toLimitGb(user.storage.quota_bytes));
      toast({
        title: user.storage.quota_bytes === null ? "Limit removed" : "Storage limit saved",
        tone: "success",
      });
    },
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (dirty && parsed !== undefined) save.mutate(parsed);
  };

  return (
    <SettingsSection
      id="storage"
      title="Storage"
      description="What your files take up, and a limit you can set for yourself so a runaway import cannot fill the disk."
    >
      <Card>
        <form onSubmit={submit}>
          <CardContent className="grid gap-5">
            <FormError message={save.isError ? sentence(save.error.message) : null} />
            <div className="grid gap-2">
              {quota === null ? (
                <p className="text-sm text-fg">{formatBytes(used)} stored, with no limit set.</p>
              ) : (
                <>
                  <Progress
                    value={quota > 0 ? used / quota : used > 0 ? 1 : 0}
                    label="Storage used"
                  />
                  <p className="text-sm text-fg">
                    {formatBytes(used)} of {formatBytes(quota)} used
                    {over ? null : ` · ${formatBytes(remaining ?? 0)} left`}
                  </p>
                </>
              )}
              {over ? (
                <p className="text-xs text-danger">
                  You are over your limit. Nothing was deleted, but uploads are refused until you
                  free space or raise the limit.
                </p>
              ) : null}
            </div>
            <Field
              label="Limit (GB)"
              name="storage_limit"
              type="number"
              min={0}
              step="any"
              inputMode="decimal"
              value={limit}
              onChange={(e) => setLimit(e.target.value)}
              error={parsed === undefined ? "Enter a size in GB, or leave it empty." : null}
              hint="Leave empty for no limit. Decimals are fine (0.5 is 512 MB)."
            />
          </CardContent>
          <CardFooter>
            <Button type="submit" disabled={!dirty || save.isPending}>
              {save.isPending ? "Saving…" : "Save limit"}
            </Button>
          </CardFooter>
        </form>
      </Card>
    </SettingsSection>
  );
}
