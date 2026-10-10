import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { KeyRoundIcon } from "lucide-react";
import { type FormEvent, useState } from "react";
import { unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import {
  type ApiToken,
  type CreatedToken,
  type TokenState,
  tokenKeys,
  tokenState,
  tokensQuery,
} from "@/api/tokens";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter } from "@/components/ui/card";
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
import { Segmented } from "@/components/ui/segmented";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { formatDateTime, formatRelative } from "@/lib/format";
import { sentence } from "@/lib/session";
import { FormError } from "@/routes/auth/form-error";
import { SettingsSection } from "./settings-section";
import { TokenCreatedDialog } from "./token-created-dialog";

type Access = "read" | "write";
type Expiry = "30" | "90" | "365" | "never";

const ACCESS = [
  { value: "read", label: "Read only" },
  { value: "write", label: "Read and write" },
] as const;

const EXPIRY = [
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
  { value: "365", label: "1 year" },
  { value: "never", label: "Never" },
] as const;

const STATE: Record<TokenState, { label: string; tone: "success" | "neutral" | "danger" }> = {
  active: { label: "Active", tone: "success" },
  expired: { label: "Expired", tone: "neutral" },
  revoked: { label: "Revoked", tone: "danger" },
};

function CreateTokenForm({ onCreated }: { onCreated: (token: CreatedToken) => void }) {
  const api = useApi();
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [access, setAccess] = useState<Access>("read");
  const [expiry, setExpiry] = useState<Expiry>("90");
  const create = useMutation({
    mutationFn: () =>
      unwrap(
        api.POST("/api/v1/me/tokens", {
          body: {
            name: name.trim(),
            scopes: access === "write" ? ["read", "write"] : ["read"],
            expires_in_days: expiry === "never" ? null : Number(expiry),
          },
        }),
      ),
    onSuccess: (created) => {
      setName("");
      void queryClient.invalidateQueries({ queryKey: tokenKeys.all });
      onCreated(created);
    },
  });
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim()) create.mutate();
  };
  return (
    <Card>
      <form onSubmit={submit}>
        <CardContent className="grid gap-5">
          <FormError message={create.isError ? sentence(create.error.message) : null} />
          <Field
            label="Name"
            name="token_name"
            autoComplete="off"
            maxLength={100}
            required
            placeholder="Claude Desktop on my laptop"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <div className="grid gap-1.5">
            <span className="text-sm font-medium text-fg">Access</span>
            <Segmented label="Access" value={access} onValueChange={setAccess} options={ACCESS} />
            <p className="text-xs text-fg-subtle">
              {access === "read"
                ? "Search, read and ask about your files. Recommended."
                : "Also add notes, tag, upload and delete files."}
            </p>
          </div>
          <div className="grid gap-1.5">
            <span className="text-sm font-medium text-fg">Expires</span>
            <Segmented label="Expires" value={expiry} onValueChange={setExpiry} options={EXPIRY} />
          </div>
        </CardContent>
        <CardFooter>
          <Button type="submit" disabled={!name.trim() || create.isPending}>
            <KeyRoundIcon aria-hidden />
            {create.isPending ? "Creating…" : "Create token"}
          </Button>
        </CardFooter>
      </form>
    </Card>
  );
}

function RevokeButton({ token }: { token: ApiToken }) {
  const api = useApi();
  const queryClient = useQueryClient();
  const revoke = useMutation({
    mutationFn: () =>
      unwrap(api.DELETE("/api/v1/me/tokens/{id}", { params: { path: { id: token.id } } })),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: tokenKeys.all });
      toast({ title: `Revoked “${token.name}”`, tone: "success" });
    },
    onError: (error) => toast({ title: sentence(error.message), tone: "danger" }),
  });
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="secondary" size="sm" aria-label={`Revoke ${token.name}`}>
          Revoke
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Revoke “{token.name}”?</DialogTitle>
          <DialogDescription>
            Apps using it lose access immediately. This can't be undone.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose asChild>
            <Button variant="secondary">Keep it</Button>
          </DialogClose>
          <DialogClose asChild>
            <Button variant="danger" onClick={() => revoke.mutate()}>
              Revoke token
            </Button>
          </DialogClose>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function TokenRow({ token }: { token: ApiToken }) {
  const state = tokenState(token);
  const { label, tone } = STATE[state];
  const write = token.scopes.includes("write");
  return (
    <li className="grid gap-2 border-b border-border py-4 first:pt-0 last:border-b-0 last:pb-0 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center sm:gap-4">
      <div className="grid min-w-0 gap-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="truncate font-medium text-fg">{token.name}</span>
          <Badge tone={tone}>{label}</Badge>
          <Badge tone={write ? "accent" : "neutral"}>{write ? "Read + write" : "Read only"}</Badge>
        </div>
        <p className="text-xs text-fg-muted">
          <span className="font-mono">{token.prefix}…</span>
          {" · "}created{" "}
          <time dateTime={token.created_at} title={formatDateTime(token.created_at)}>
            {formatRelative(token.created_at)}
          </time>
          {" · "}
          {token.last_used_at ? `last used ${formatRelative(token.last_used_at)}` : "never used"}
          {token.expires_at && state === "active"
            ? ` · expires ${formatRelative(token.expires_at)}`
            : null}
        </p>
      </div>
      {state === "active" ? <RevokeButton token={token} /> : null}
    </li>
  );
}

/** Settings → Access tokens: create, copy once, list and revoke API tokens. */
export function TokensPanel() {
  const api = useApi();
  const tokens = useQuery(tokensQuery(api));
  const [created, setCreated] = useState<CreatedToken | null>(null);
  return (
    <>
      <SettingsSection
        id="new-token"
        title="New access token"
        description={
          <>
            Tokens let AI assistants (Claude Code, Claude Desktop and other MCP clients) and scripts
            use your library. A token can read everything in it, so prefer read-only tokens with an
            expiry, and revoke any you no longer use.
          </>
        }
      >
        <CreateTokenForm onCreated={setCreated} />
      </SettingsSection>
      <SettingsSection
        id="tokens"
        title="Your tokens"
        description="Tokens can't manage your account or other tokens."
      >
        <Card>
          <CardContent>
            {tokens.isPending ? (
              <Skeleton className="h-12" />
            ) : tokens.isError ? (
              <p className="text-sm text-danger">{sentence(tokens.error.message)}</p>
            ) : tokens.data.items.length === 0 ? (
              <p className="text-sm text-fg-muted">No tokens yet.</p>
            ) : (
              <ul aria-label="Access tokens">
                {tokens.data.items.map((t) => (
                  <TokenRow key={t.id} token={t} />
                ))}
              </ul>
            )}
          </CardContent>
        </Card>
      </SettingsSection>
      <TokenCreatedDialog created={created} onClose={() => setCreated(null)} />
    </>
  );
}
