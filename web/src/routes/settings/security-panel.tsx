import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { LogOutIcon, MonitorIcon, ShieldAlertIcon, SmartphoneIcon } from "lucide-react";
import { useId, useState } from "react";
import { activityKeys, type SessionInfo, sessionsQuery, signInsQuery } from "@/api/activity";
import { type User, unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { keys } from "@/api/queries";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { formatDateTime, formatRelative } from "@/lib/format";
import { sentence } from "@/lib/session";
import { describeAgent } from "@/lib/user-agent";
import { SettingsSection } from "./settings-section";

function isPhone(ua: string | null | undefined): boolean {
  return Boolean(ua && /iPhone|Android|Mobile/.test(ua));
}

/** Settings → Security: sessions, recent sign-ins and search history. */
export function SecurityPanel({ me }: { me: User }) {
  return (
    <>
      <SessionsSection />
      <SignInsSection />
      <SearchHistorySection me={me} />
    </>
  );
}

function SessionsSection() {
  const api = useApi();
  const queryClient = useQueryClient();
  const sessions = useQuery(sessionsQuery(api));
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: activityKeys.sessions });
    void queryClient.invalidateQueries({ queryKey: activityKeys.all });
  };
  const revoke = useMutation({
    mutationFn: (id: string) =>
      unwrap(api.DELETE("/api/v1/me/sessions/{id}", { params: { path: { id } } })),
    onSuccess: () => {
      refresh();
      toast({ title: "Signed out that session", tone: "success" });
    },
    onError: (e) => toast({ title: sentence(e.message), tone: "danger" }),
  });
  const revokeOthers = useMutation({
    mutationFn: () => unwrap(api.POST("/api/v1/me/sessions/revoke-others")),
    onSuccess: (res) => {
      refresh();
      toast({
        title:
          res.revoked === 0
            ? "No other sessions"
            : `Signed out ${res.revoked === 1 ? "1 other session" : `${res.revoked} other sessions`}`,
        tone: "success",
      });
    },
    onError: (e) => toast({ title: sentence(e.message), tone: "danger" }),
  });
  const items = sessions.data?.items ?? [];
  const others = items.filter((s) => !s.current).length;

  return (
    <SettingsSection
      id="sessions"
      title="Where you're signed in"
      description="Each browser you signed in with. Sign out any you don't recognise, then change your password."
    >
      {sessions.isPending ? (
        <Skeleton className="h-28" />
      ) : sessions.isError ? (
        <p role="alert" className="text-sm text-danger">
          Couldn't load your sessions.
        </p>
      ) : (
        <div className="grid gap-4">
          <ul className="divide-y divide-border rounded-lg border border-border bg-surface">
            {items.map((s) => (
              <SessionRow
                key={s.id}
                session={s}
                busy={revoke.isPending && revoke.variables === s.id}
                onRevoke={() => revoke.mutate(s.id)}
              />
            ))}
          </ul>
          <div>
            <Button
              variant="secondary"
              onClick={() => revokeOthers.mutate()}
              disabled={others === 0 || revokeOthers.isPending}
            >
              <LogOutIcon /> Sign out other sessions
            </Button>
          </div>
        </div>
      )}
    </SettingsSection>
  );
}

function SessionRow({
  session,
  busy,
  onRevoke,
}: {
  session: SessionInfo;
  busy: boolean;
  onRevoke: () => void;
}) {
  const Icon = isPhone(session.user_agent) ? SmartphoneIcon : MonitorIcon;
  const browser = describeAgent(session.user_agent);
  return (
    <li className="flex items-center gap-3 px-4 py-3">
      <Icon className="size-5 shrink-0 text-fg-subtle" aria-hidden strokeWidth={1.5} />
      <div className="grid min-w-0 flex-1 gap-0.5">
        <p className="flex flex-wrap items-center gap-2 text-sm text-fg">
          <span className="truncate font-medium">{browser}</span>
          {session.current ? <Badge tone="accent">This device</Badge> : null}
        </p>
        <p className="text-xs text-fg-subtle">
          Signed in {session.ip ? `from ${session.ip} ` : ""}
          <time dateTime={session.created_at} title={formatDateTime(session.created_at)}>
            {formatRelative(session.created_at)}
          </time>
          {" · "}last active{" "}
          <time dateTime={session.last_seen_at} title={formatDateTime(session.last_seen_at)}>
            {formatRelative(session.last_seen_at)}
          </time>
        </p>
      </div>
      {session.current ? null : (
        <Button
          variant="ghost"
          size="sm"
          onClick={onRevoke}
          disabled={busy}
          aria-label={`Sign out ${browser}`}
        >
          Sign out
        </Button>
      )}
    </li>
  );
}

function SignInsSection() {
  const api = useApi();
  const signIns = useQuery(signInsQuery(api));
  const items = signIns.data?.items ?? [];
  return (
    <SettingsSection
      id="sign-ins"
      title="Recent sign-ins"
      description={
        <>
          Successful and failed sign-ins to your account, with the address and browser.{" "}
          <Link
            to="/activity"
            search={{ category: "security" }}
            className="text-accent-text underline underline-offset-2 hover:decoration-2"
          >
            Full security log
          </Link>
        </>
      }
    >
      {signIns.isPending ? (
        <Skeleton className="h-24" />
      ) : items.length === 0 ? (
        <p className="text-sm text-fg-subtle">No sign-ins recorded yet.</p>
      ) : (
        <ul className="grid gap-2.5">
          {items.map((e) => {
            const failed = e.kind === "auth.sign_in_failed";
            return (
              <li key={e.id} className="flex items-start gap-2.5 text-sm">
                {failed ? (
                  <ShieldAlertIcon className="mt-0.5 size-4 shrink-0 text-danger" aria-hidden />
                ) : (
                  <span
                    className="mt-2 size-1.5 shrink-0 rounded-full bg-border-strong"
                    aria-hidden
                  />
                )}
                <p className={failed ? "text-danger" : "text-fg-muted"}>
                  {failed
                    ? "Failed attempt"
                    : e.kind === "account.created"
                      ? "Created the account"
                      : "Signed in"}
                  {e.ip ? (
                    <>
                      {" "}
                      from <span className="font-mono text-xs">{e.ip}</span>
                    </>
                  ) : null}{" "}
                  · {describeAgent(e.user_agent)} ·{" "}
                  <time dateTime={e.created_at} title={formatDateTime(e.created_at)}>
                    {formatRelative(e.created_at)}
                  </time>
                </p>
              </li>
            );
          })}
        </ul>
      )}
    </SettingsSection>
  );
}

function SearchHistorySection({ me }: { me: User }) {
  const api = useApi();
  const queryClient = useQueryClient();
  const id = useId();
  const save = useMutation({
    mutationFn: (on: boolean) =>
      unwrap(api.PATCH("/api/v1/me", { body: { record_search_history: on } })),
    onSuccess: (user) => {
      queryClient.setQueryData(keys.me, user);
      toast({
        title: user.record_search_history ? "Searches will be kept" : "Searches won't be kept",
        tone: "success",
      });
    },
    onError: (e) => toast({ title: sentence(e.message), tone: "danger" }),
  });
  const clear = useMutation({
    mutationFn: () =>
      unwrap(api.DELETE("/api/v1/activity", { params: { query: { category: "search" } } })),
    onSuccess: (res) => {
      void queryClient.invalidateQueries({ queryKey: activityKeys.all });
      toast({
        title: res.deleted === 1 ? "Cleared 1 search" : `Cleared ${res.deleted} searches`,
        tone: "success",
      });
    },
    onError: (e) => toast({ title: sentence(e.message), tone: "danger" }),
  });
  // Optimistic: the box follows the click at once, and goes back if saving fails.
  const [on, setOn] = useState(me.record_search_history);
  return (
    <SettingsSection
      id="search-history"
      title="Search history"
      description="Your searches, with what you typed, appear in your activity so you can find them again. Only you can see them."
    >
      <div className="grid gap-4">
        <label htmlFor={id} className="flex cursor-pointer items-start gap-3">
          <input
            id={id}
            type="checkbox"
            className="mt-0.5 size-4 shrink-0 accent-[var(--accent)]"
            checked={on}
            onChange={(e) => {
              const next = e.currentTarget.checked;
              setOn(next);
              save.mutate(next, { onError: () => setOn(!next) });
            }}
          />
          <span className="grid gap-0.5">
            <span className="text-sm font-medium text-fg">Keep a history of my searches</span>
            <span className="text-xs text-fg-subtle">
              When off, searches are not recorded at all. Questions you ask stay in their
              conversations either way.
            </span>
          </span>
        </label>
        <div>
          <Button variant="secondary" onClick={() => clear.mutate()} disabled={clear.isPending}>
            Clear search history
          </Button>
        </div>
      </div>
    </SettingsSection>
  );
}
