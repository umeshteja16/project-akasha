// How each activity event reads in the timeline: an icon, a sentence with links,
// and a tone (security problems stand out).

import { Link } from "@tanstack/react-router";
import {
  BanIcon,
  FileInputIcon,
  FilePenLineIcon,
  FileXIcon,
  FolderMinusIcon,
  FolderPenIcon,
  FolderPlusIcon,
  FolderSyncIcon,
  FolderXIcon,
  KeyRoundIcon,
  LogInIcon,
  LogOutIcon,
  type LucideIcon,
  MessageSquareQuoteIcon,
  MonitorXIcon,
  OctagonAlertIcon,
  ScanEyeIcon,
  SearchIcon,
  ShieldAlertIcon,
  ShieldCheckIcon,
  TagIcon,
  UploadIcon,
  UserPlusIcon,
} from "lucide-react";
import type { ReactNode } from "react";
import type { ActivityItem, ActivityKind } from "@/api/activity";
import { describeAgent } from "@/lib/user-agent";

export interface Described {
  icon: LucideIcon;
  text: ReactNode;
  /** `alert`: a failed or refused security action. */
  tone?: "alert";
  /** Extra line under the sentence (address and browser, file names). */
  meta?: string;
}

function str(value: unknown): string | undefined {
  return typeof value === "string" && value ? value : undefined;
}

function num(value: unknown): number | undefined {
  return typeof value === "number" ? value : undefined;
}

function strings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
}

const strong = "font-medium text-fg";
const linkClass =
  "font-medium text-fg underline decoration-border-strong underline-offset-2 hover:decoration-fg";

function FileName({ item, name }: { item: ActivityItem; name?: string }) {
  const label = name ?? item.subject ?? "a file";
  return item.file_id ? (
    <Link to="/files/$fileId" params={{ fileId: item.file_id }} className={linkClass}>
      {label}
    </Link>
  ) : (
    <span className={strong}>{label}</span>
  );
}

function CollectionName({ item, name }: { item: ActivityItem; name?: string }) {
  const label = name ?? item.subject ?? "a collection";
  return item.collection_id ? (
    <Link
      to="/collections/$collectionId"
      params={{ collectionId: item.collection_id }}
      className={linkClass}
    >
      {label}
    </Link>
  ) : (
    <span className={strong}>{label}</span>
  );
}

function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

function where(item: ActivityItem): string {
  return [
    item.ip ? `from ${item.ip}` : null,
    item.user_agent ? describeAgent(item.user_agent) : null,
  ]
    .filter(Boolean)
    .join(" · ");
}

/** A short list of names: "a.pdf, b.txt and 3 more". */
function names(list: string[], total: number): string | undefined {
  if (list.length === 0) return undefined;
  const more = total - list.length;
  return more > 0 ? `${list.join(", ")} and ${more} more` : list.join(", ");
}

type Describer = (item: ActivityItem, d: Record<string, unknown>) => Described;

const DESCRIBE: Record<ActivityKind, Describer> = {
  "file.uploaded": (item) => ({
    icon: UploadIcon,
    text: (
      <>
        Added <FileName item={item} />
      </>
    ),
  }),
  "file.renamed": (item, d) => ({
    icon: FilePenLineIcon,
    text: (
      <>
        Renamed <span className={strong}>{str(d.from) ?? "a file"}</span> to{" "}
        <FileName item={item} name={str(d.to)} />
      </>
    ),
  }),
  "file.tagged": (item, d) => {
    const added = strings(d.added);
    const removed = strings(d.removed);
    return {
      icon: TagIcon,
      text: (
        <>
          {added.length ? <>Tagged </> : <>Untagged </>}
          <FileName item={item} />
          {added.length ? <> with {added.join(", ")}</> : null}
        </>
      ),
      meta: removed.length ? `Removed ${removed.join(", ")}` : undefined,
    };
  },
  "file.deleted": (item, d) => {
    const count = num(d.count) ?? 1;
    return {
      icon: FileXIcon,
      text:
        count > 1 ? (
          <>Deleted {plural(count, "file")}</>
        ) : (
          <>
            Deleted <span className={strong}>{item.subject ?? "a file"}</span>
          </>
        ),
      meta: count > 1 ? names(strings(d.file_names), count) : undefined,
    };
  },
  "file.opened": (item) => ({
    icon: FileInputIcon,
    text: (
      <>
        Opened <FileName item={item} />
      </>
    ),
  }),
  "source.added": (item) => ({
    icon: FolderSyncIcon,
    text: (
      <>
        Started watching <span className={strong}>{item.subject ?? "a folder"}</span>
      </>
    ),
  }),
  "source.removed": (item, d) => {
    const deleted = num(d.deleted_files) ?? 0;
    return {
      icon: FolderXIcon,
      text: (
        <>
          Stopped watching <span className={strong}>{item.subject ?? "a folder"}</span>
        </>
      ),
      meta: deleted ? `Deleted ${plural(deleted, "imported file")}` : undefined,
    };
  },
  "source.synced": (item, d) => {
    const parts = [
      [num(d.imported) ?? 0, "added"],
      [num(d.updated) ?? 0, "updated"],
      [num(d.removed) ?? 0, "removed"],
    ] as const;
    return {
      icon: FolderSyncIcon,
      text: (
        <>
          Synced <span className={strong}>{item.subject ?? "a folder"}</span>
        </>
      ),
      meta: parts
        .filter(([n]) => n > 0)
        .map(([n, what]) => `${plural(n, "file")} ${what}`)
        .join(" · "),
    };
  },
  "search.performed": (item, d) => {
    const results = num(d.results);
    return {
      icon: SearchIcon,
      text: (
        <>
          Searched for{" "}
          <Link to="/search" search={{ q: item.subject ?? "" }} className={linkClass}>
            “{item.subject}”
          </Link>
        </>
      ),
      meta:
        results === undefined
          ? undefined
          : results === 0
            ? "Nothing found"
            : `${plural(results, "file")}${d.more === true ? " or more" : ""} found`,
    };
  },
  "chat.asked": (item) => ({
    icon: MessageSquareQuoteIcon,
    text: (
      <>
        Asked{" "}
        {item.conversation_id ? (
          <Link
            to="/chat/$conversationId"
            params={{ conversationId: item.conversation_id }}
            className={linkClass}
          >
            “{item.subject}”
          </Link>
        ) : (
          <span className={strong}>“{item.subject}”</span>
        )}
      </>
    ),
  }),
  "collection.created": (item) => ({
    icon: FolderPlusIcon,
    text: (
      <>
        Created the collection <CollectionName item={item} />
      </>
    ),
  }),
  "collection.updated": (item, d) => ({
    icon: FolderPenIcon,
    text: str(d.from) ? (
      <>
        Renamed the collection <span className={strong}>{str(d.from)}</span> to{" "}
        <CollectionName item={item} name={str(d.to)} />
      </>
    ) : (
      <>
        Updated the collection <CollectionName item={item} />
      </>
    ),
  }),
  "collection.deleted": (item) => ({
    icon: FolderXIcon,
    text: (
      <>
        Deleted the collection <span className={strong}>{item.subject}</span>
      </>
    ),
  }),
  "collection.files_added": (item, d) => {
    const count = num(d.count) ?? 1;
    const list = strings(d.file_names);
    return {
      icon: FolderPlusIcon,
      text: (
        <>
          Added {count === 1 ? <FileName item={item} name={list[0]} /> : plural(count, "file")} to{" "}
          <CollectionName item={item} />
        </>
      ),
      meta: count > 1 ? names(list, count) : undefined,
    };
  },
  "collection.files_removed": (item, d) => {
    const count = num(d.count) ?? 1;
    const list = strings(d.file_names);
    return {
      icon: FolderMinusIcon,
      text: (
        <>
          Removed {count === 1 ? <FileName item={item} name={list[0]} /> : plural(count, "file")}{" "}
          from <CollectionName item={item} />
        </>
      ),
      meta: count > 1 ? names(list, count) : undefined,
    };
  },
  "account.created": () => ({ icon: UserPlusIcon, text: "Created your account" }),
  "auth.signed_in": (item) => ({ icon: LogInIcon, text: "Signed in", meta: where(item) }),
  "auth.sign_in_failed": (item) => ({
    icon: ShieldAlertIcon,
    tone: "alert",
    text: "Failed sign-in attempt (wrong password)",
    meta: where(item),
  }),
  "auth.signed_out": (item) => ({ icon: LogOutIcon, text: "Signed out", meta: where(item) }),
  "auth.password_changed": (item, d) => {
    const revoked = num(d.sessions_revoked) ?? 0;
    return {
      icon: ShieldCheckIcon,
      text: "Changed your password",
      meta: [revoked ? `Signed out ${plural(revoked, "other session")}` : null, where(item)]
        .filter(Boolean)
        .join(" · "),
    };
  },
  "auth.password_change_failed": (item) => ({
    icon: ShieldAlertIcon,
    tone: "alert",
    text: "Password not changed: the current password was wrong",
    meta: where(item),
  }),
  "account.delete_failed": (item) => ({
    icon: ShieldAlertIcon,
    tone: "alert",
    text: "Account not deleted: the password was wrong",
    meta: where(item),
  }),
  "session.revoked": (item, d) => {
    const count = num(d.count) ?? 1;
    return {
      icon: MonitorXIcon,
      text:
        d.others === true
          ? `Signed out everywhere else (${plural(count, "session")})`
          : "Signed out another session",
      meta: where(item),
    };
  },
  "token.created": (item, d) => ({
    icon: KeyRoundIcon,
    text: (
      <>
        Created the API token <span className={strong}>{item.subject}</span>
      </>
    ),
    meta: d.write === true ? "Read and write access" : "Read-only access",
  }),
  "token.revoked": (item) => ({
    icon: BanIcon,
    text: (
      <>
        Revoked the API token <span className={strong}>{item.subject}</span>
      </>
    ),
  }),
  "rate.limited": (item) => ({
    icon: OctagonAlertIcon,
    tone: "alert",
    text: `Slowed down: too many ${item.subject ?? "requests"} in a short time`,
    meta: item.via === "token" ? "From an API token or AI assistant" : undefined,
  }),
};

export function describe(item: ActivityItem): Described {
  const details = (item.details ?? {}) as Record<string, unknown>;
  const fn = DESCRIBE[item.kind];
  return fn ? fn(item, details) : { icon: ScanEyeIcon, text: item.kind };
}
